/// Copyright (C) 2026 Fazal Majid
///
/// This program is free software: you can redistribute it and/or modify
/// it under the terms of the GNU Affero General Public License as published by
/// the Free Software Foundation, either version 3 of the License, or
/// (at your option) any later version.
///
/// This program is distributed in the hope that it will be useful,
/// but WITHOUT ANY WARRANTY; without even the implied warranty of
/// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
/// GNU Affero General Public License for more details.
///
/// You should have received a copy of the GNU Affero General Public License
/// along with this program.  If not, see <https://www.gnu.org/licenses/>.
///
use crate::feeds::normalize::Item;
use hyperscan_tokio::prelude::*;
use log::warn;
use regex::escape;
use rust_stemmers::{Algorithm, Stemmer};
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;

#[repr(u8)]
#[derive(Clone, Debug)]
pub enum RuleType {
    Author,
    ContentPhrase,
    ContentPhraseLowerCase,
    Python,
    Tag,
    TitleAll,
    TitleExactWord,
    TitlePhrase,
    TitlePhraseLowerCase,
    TitleWord,
    UnionAll,
    UnionExactWord,
    UnionPhrase,
    UnionPhraseLowerCase,
    UnionWord,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub uid: u32,
    pub rule_type: RuleType,
    pub text: String,
    pub feed: Option<u32>,
    pub feed_title: Option<String>,
}

impl fmt::Display for RuleType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleType::Author => write!(f, "Author"),
            RuleType::ContentPhrase => write!(f, "ContentPhrase"),
            RuleType::ContentPhraseLowerCase => write!(f, "ContentPhraseLowerCase"),
            RuleType::Python => write!(f, "Python"),
            RuleType::Tag => write!(f, "Tag"),
            RuleType::TitleAll => write!(f, "TitleAll"),
            RuleType::TitleExactWord => write!(f, "TitleExactWord"),
            RuleType::TitlePhrase => write!(f, "TitlePhrase"),
            RuleType::TitlePhraseLowerCase => write!(f, "TitlePhraseLowerCase"),
            RuleType::TitleWord => write!(f, "TitleWord"),
            RuleType::UnionAll => write!(f, "UnionAll"),
            RuleType::UnionExactWord => write!(f, "UnionExactWord"),
            RuleType::UnionPhrase => write!(f, "UnionPhrase"),
            RuleType::UnionPhraseLowerCase => write!(f, "UnionPhraseLowerCase"),
            RuleType::UnionWord => write!(f, "UnionWord"),
        }
    }
}

pub fn rule_from_string(s: String) -> RuleType {
    match s.as_str() {
        "author" => RuleType::Author,
        "content_phrase" => RuleType::ContentPhrase,
        "content_phrase_lc" => RuleType::ContentPhraseLowerCase,
        "python" => RuleType::Python,
        "tag" => RuleType::Tag,
        "title_all" => RuleType::TitleAll,
        "title_exactword" => RuleType::TitleExactWord,
        "title_phrase" => RuleType::TitlePhrase,
        "title_phrase_lc" => RuleType::TitlePhraseLowerCase,
        "title_word" => RuleType::TitleWord,
        "union_all" => RuleType::UnionAll,
        "union_exactword" => RuleType::UnionExactWord,
        "union_phrase" => RuleType::UnionPhrase,
        "union_phrase_lc" => RuleType::UnionPhraseLowerCase,
        "union_word" => RuleType::UnionWord,
        _ => RuleType::UnionPhrase,
    }
}

struct Matcher {
    regex_db: Option<DatabaseBuilder>,
    regex_db_empty: bool,
    regex: Option<Scanner>,
    word_stem: HashMap<String, u32>,
    word_exact: HashMap<String, u32>,
    word_all: Vec<(u32, Vec<String>)>,
}

fn new_matcher() -> Matcher {
    Matcher {
        regex_db: Some(DatabaseBuilder::new()),
        regex_db_empty: true,
        regex: None,
        word_stem: HashMap::<String, u32>::new(),
        word_exact: HashMap::<String, u32>::new(),
        word_all: Vec::<(u32, Vec<String>)>::new(),
    }
}

#[derive(Debug)]
pub enum FilterError {
    Hyperscan(hyperscan_tokio::Error),
    Db(sqlx::Error),
}

impl std::fmt::Display for FilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterError::Hyperscan(e) => write!(f, "Hyperscan error: {}", e),
            FilterError::Db(e) => write!(f, "sqlx DB error: {}", e),
        }
    }
}

impl std::error::Error for FilterError {}

impl From<hyperscan_tokio::Error> for FilterError {
    fn from(err: hyperscan_tokio::Error) -> Self {
        FilterError::Hyperscan(err)
    }
}
impl From<sqlx::Error> for FilterError {
    fn from(err: sqlx::Error) -> Self {
        FilterError::Db(err)
    }
}

fn escape_plain(s: String) -> PatternBuilder {
    // Hyperscan does not have a concept of plain string and always
    // interprets it as a regex, so we have to escape all regex
    // meta characters
    Pattern::new(escape(&s))
}

impl Matcher {
    fn add_pattern(
        &mut self,
        rule_text: String,
        rule_uid: u32,
        case_sensitive: bool,
    ) -> std::result::Result<(), FilterError> {
        self.regex_db_empty = false;
        match case_sensitive {
            true => {
                self.regex_db = Some(
                    self.regex_db
                        .take()
                        .expect("uninitialized regex_db")
                        .add_pattern(escape_plain(rule_text).id(rule_uid).build()?),
                );
            }
            false => {
                self.regex_db = Some(
                    self.regex_db
                        .take()
                        .expect("uninitialized regex_db")
                        .add_pattern(
                            escape_plain(rule_text.to_lowercase())
                                .flags(Flags::CASELESS)
                                .id(rule_uid)
                                .build()?,
                        ),
                );
            }
        };
        Ok(())
    }
    fn finalize(&mut self) -> Result<()> {
        if !self.regex_db_empty
            && let Some(db_builder) = self.regex_db.take()
        {
            let db = db_builder.build()?;
            self.regex = Some(Scanner::new(db)?);
        }
        Ok(())
    }
    pub async fn apply(
        &self,
        text: &str,
        words: &Vec<String>,
        words_stem: &Vec<String>,
    ) -> std::result::Result<Option<u32>, FilterError> {
        for word in words {
            if let Some(uid) = self.word_exact.get(word) {
                return Ok(Some(*uid));
            }
        }
        let mut set = HashSet::new();
        for word in words_stem {
            set.insert(word);
            if let Some(uid) = self.word_stem.get(word) {
                return Ok(Some(*uid));
            }
        }
        for (uid, combo) in self.word_all.clone() {
            if combo.iter().all(|word| set.contains(word)) {
                return Ok(Some(uid));
            }
        }
        match &self.regex {
            Some(regex) => match regex
                .scan_bytes(text.to_owned().into_bytes())
                .await?
                .first()
            {
                Some(m) => Ok(Some(m.pattern_id)),
                None => Ok(None),
            },
            None => Ok(None),
        }
    }
}

struct RuleSet {
    title: Matcher,
    content: Matcher,
    tag: HashMap<String, u32>,
    author: HashMap<String, u32>,
}

fn new_ruleset() -> RuleSet {
    RuleSet {
        title: new_matcher(),
        content: new_matcher(),
        tag: HashMap::<String, u32>::new(),
        author: HashMap::<String, u32>::new(),
    }
}

impl RuleSet {
    fn finalize(&mut self) -> Result<()> {
        self.title.finalize()?;
        self.content.finalize()
    }
    pub async fn apply(&self, item: &Item) -> std::result::Result<Option<u32>, FilterError> {
        let en_stemmer: Stemmer = Stemmer::create(Algorithm::English);

        let title_words = item
            .title
            .split_whitespace()
            .map(|w| w.to_string())
            .collect();
        let title_stem = item
            .title
            .to_lowercase()
            .split_whitespace()
            .map(|w| en_stemmer.stem(w).into_owned())
            .collect();
        if let Some(uid) = self
            .title
            .apply(&item.title, &title_words, &title_stem)
            .await?
        {
            return Ok(Some(uid));
        }
        let content_words = item
            .content
            .split_whitespace()
            .map(|w| w.to_string())
            .collect();
        let content_stem = item
            .content
            .to_lowercase()
            .split_whitespace()
            .map(|w| en_stemmer.stem(w).into_owned())
            .collect();
        if let Some(uid) = self
            .content
            .apply(&item.content, &content_words, &content_stem)
            .await?
        {
            return Ok(Some(uid));
        }
        if let Some(uid) = self.author.get(&item.author) {
            return Ok(Some(*uid));
        }
        for tag in &item.tags {
            if let Some(uid) = self.tag.get(tag) {
                return Ok(Some(*uid));
            }
        }

        Ok(None)
    }
}

pub struct Filters {
    global: RuleSet,
    per_feed: HashMap<u32, RuleSet>,
    rules: HashMap<u32, Rule>,
}

pub fn new_filters() -> Filters {
    Filters {
        global: new_ruleset(),
        per_feed: HashMap::new(),
        rules: HashMap::new(),
    }
}

impl Filters {
    pub fn add_rule(
        &mut self,
        feed: Option<i64>,
        rule: Rule,
    ) -> std::result::Result<&Filters, FilterError> {
        self.rules.insert(rule.uid, rule.clone());
        let en_stemmer: Stemmer = Stemmer::create(Algorithm::English);

        let f = match feed {
            Some(uid) => self.per_feed.entry(uid as u32).or_insert_with(new_ruleset),
            _ => &mut self.global,
        };
        match rule.rule_type {
            RuleType::Author => {
                f.author.insert(rule.text.clone(), rule.uid);
            }
            RuleType::ContentPhrase => f.content.add_pattern(rule.text, rule.uid, true)?,
            RuleType::ContentPhraseLowerCase => {
                f.content.add_pattern(rule.text, rule.uid, false)?
            }
            RuleType::Tag => {
                f.tag.insert(rule.text.clone(), rule.uid);
            }
            RuleType::TitleExactWord => {
                f.title.word_exact.insert(rule.text.clone(), rule.uid);
            }
            RuleType::TitlePhrase => f.title.add_pattern(rule.text, rule.uid, true)?,
            RuleType::TitlePhraseLowerCase => f.title.add_pattern(rule.text, rule.uid, false)?,
            RuleType::TitleWord => {
                f.title.word_stem.insert(
                    en_stemmer.stem(&rule.text.to_lowercase()).to_string(),
                    rule.uid,
                );
            }
            RuleType::TitleAll => {
                f.title.word_all.push((
                    rule.uid,
                    rule.text
                        .split_whitespace()
                        .map(|w| en_stemmer.stem(w).to_string())
                        .collect(),
                ));
            }
            RuleType::UnionExactWord => {
                f.title.word_exact.insert(rule.text.clone(), rule.uid);
                f.content.word_exact.insert(rule.text.clone(), rule.uid);
            }
            RuleType::UnionPhrase => {
                f.title.add_pattern(rule.text.clone(), rule.uid, true)?;
                f.content.add_pattern(rule.text.clone(), rule.uid, true)?
            }
            RuleType::UnionPhraseLowerCase => {
                f.title.add_pattern(rule.text.clone(), rule.uid, true)?;
                f.content.add_pattern(rule.text.clone(), rule.uid, true)?
            }
            RuleType::UnionWord => {
                f.title.word_stem.insert(
                    en_stemmer.stem(&rule.text.to_lowercase()).to_string(),
                    rule.uid,
                );
                f.content.word_stem.insert(
                    en_stemmer.stem(&rule.text.to_lowercase()).to_string(),
                    rule.uid,
                );
            }
            RuleType::UnionAll => {
                f.title.word_all.push((
                    rule.uid,
                    rule.text
                        .split_whitespace()
                        .map(|w| en_stemmer.stem(w).to_string())
                        .collect(),
                ));
                f.content.word_all.push((
                    rule.uid,
                    rule.text
                        .split_whitespace()
                        .map(|w| en_stemmer.stem(w).to_string())
                        .collect(),
                ));
            }
            // RuleType::Python| RuleType::TitleAll|RuleType::UnionAll
            _ => {
                warn!(
                    "rule {0} {1}({2}) not implemented",
                    rule.uid, rule.rule_type, rule.text
                );
            }
        }
        Ok(self)
    }

    pub fn finalize(&mut self) -> Result<()> {
        self.global.finalize()?;
        for rs in self.per_feed.values_mut() {
            rs.finalize()?
        }
        Ok(())
    }
    pub async fn apply_filter(
        &self,
        feed_uid: u32,
        item: &Item,
    ) -> std::result::Result<Option<u32>, FilterError> {
        // Start with feed-specific filters
        if let Some(f) = self.per_feed.get(&feed_uid)
            && let Some(uid) = f.apply(item).await?
        {
            return Ok(Some(uid));
        }
        self.global.apply(item).await
    }
}
