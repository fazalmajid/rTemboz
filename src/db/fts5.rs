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
/// Convert a Google search-like query into a FTS5 query.
///
/// Translated from Temboz' fts5.py
///
/// This prevents the FTS5 column filter misfeature by wrapping bare words
/// in quotes. We don't try to make invalid queries work, just make queries
/// that would be interpreted as column filters work correctly.
///
/// Supported syntax:
/// - `word` - bare words
/// - `"phrase"` - quoted phrases
/// - `(query)` - grouping
/// - `query AND query`
/// - `query OR query`
/// - `NOT query`
pub fn fts5_term(term: &str) -> String {
    #[derive(Clone, Copy, PartialEq)]
    enum Expect {
        None,
        WordSep,
        N,
        D,
        R,
        O,
        T,
    }

    let mut in_q = false;
    let mut implicit_q = false;
    let mut out: Vec<char> = Vec::new();
    let mut expect = Expect::None;
    let mut pending: Vec<char> = Vec::new();

    for c in term.chars() {
        // SQL injection guard
        if c == '\'' {
            if !pending.is_empty() {
                if !implicit_q {
                    out.push('"');
                    implicit_q = true;
                }
                out.extend(&pending);
                pending.clear();
            }
            out.push('\'');
            out.push('\'');
            continue;
        }

        if expect != Expect::None {
            debug_assert!(!in_q);
            let is_ws_char = matches!(c, ' ' | '\t' | '\n' | '(' | '"');

            let should_start_implicit = match expect {
                Expect::WordSep => !is_ws_char,
                Expect::N => c != 'N',
                Expect::D => c != 'D',
                Expect::R => c != 'R',
                Expect::O => c != 'O',
                Expect::T => c != 'T',
                Expect::None => unreachable!(),
            };

            if should_start_implicit {
                out.push('"');
                implicit_q = true;
                out.extend(&pending);
                expect = Expect::None;
                pending.clear();
            } else if expect == Expect::WordSep {
                out.extend(&pending);
                expect = Expect::None;
                pending.clear();
            } else {
                pending.push(c);
                expect = match expect {
                    Expect::N => Expect::D,
                    Expect::D => Expect::WordSep,
                    Expect::R => Expect::WordSep,
                    Expect::O => Expect::T,
                    Expect::T => Expect::WordSep,
                    _ => unreachable!(),
                };
                continue;
            }
        }

        if c == '"' {
            if implicit_q {
                implicit_q = false;
                in_q = true;
            } else {
                in_q = !in_q;
                out.push(c);
            }
        } else if !in_q && !implicit_q && matches!(c, 'A' | 'O' | 'N') {
            expect = match c {
                'A' => Expect::N,
                'O' => Expect::R,
                'N' => Expect::O,
                _ => unreachable!(),
            };
            pending = vec![c];
        } else if matches!(c, ' ' | '\t' | '\n' | '(' | ')') {
            if implicit_q {
                out.push('"');
                implicit_q = false;
                in_q = false;
            }
            out.push(c);
        } else if in_q || implicit_q {
            out.push(c);
        } else {
            in_q = true;
            implicit_q = true;
            out.push('"');
            out.push(c);
        }
    }

    if !pending.is_empty() {
        out.extend(&pending);
    }
    if in_q || implicit_q {
        out.push('"');
    }

    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fts5_term() {
        let cases = [
            ("foo", r#""foo""#),
            ("foo bar", r#""foo" "bar""#),
            (r#""foo""#, r#""foo""#),
            (r#""foo bar""#, r#""foo bar""#),
            ("foo AND bar", r#""foo" AND "bar""#),
            ("(foo AND bar) OR baz", r#"("foo" AND "bar") OR "baz""#),
            ("foo AN bar", r#""foo" "AN" "bar""#),
            (r#""foo AN bar""#, r#""foo AN bar""#),
            ("ACME", r#""ACME""#),
            ("Acme", r#""Acme""#),
            ("OpenBSD", r#""OpenBSD""#),
            ("ANAN", r#""ANAN""#),
            ("ANAND", r#""ANAND""#),
            (r#"AN"D""#, r#""AND""#),
            ("NOTAM", r#""NOTAM""#),
            ("ANDROS", r#""ANDROS""#),
        ];

        for (input, expected) in cases {
            let result = fts5_term(input);
            assert_eq!(
                result, expected,
                "input={:?} expected={:?} got={:?}",
                input, expected, result
            );
        }
    }
}
