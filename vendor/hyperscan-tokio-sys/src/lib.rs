//! Low-level FFI bindings to VectorScan/Hyperscan
//! 
//! This crate provides raw bindings and safe wrappers around the VectorScan C API.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::{c_char, c_int, c_uint, c_void, c_ulonglong};
use std::ptr;
use std::slice;

// Include generated bindings
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

// Re-export key types
pub use hs_database_t as RawDatabase;
pub use hs_stream_t as RawStream;
pub use hs_scratch_t as RawScratch;
pub use hs_compile_error_t as RawCompileError;

// Chimera types - only available when has_chimera is set
#[cfg(has_chimera)]
pub use ch_database_t as RawChimeraDatabase;
#[cfg(has_chimera)]
pub use ch_scratch_t as RawChimeraScratch;
#[cfg(has_chimera)]
pub use ch_compile_error_t as RawChimeraCompileError;

/// Database pointer wrapper
/// 
/// SAFETY INVARIANTS:
/// - Inner pointer is always valid or null
/// - Pointer is freed exactly once in Drop
/// - Send + Sync safe because VectorScan databases are thread-safe
#[repr(transparent)]
pub struct DatabasePtr(pub *mut hs_database_t);

// SAFETY: VectorScan databases are thread-safe for read operations
unsafe impl Send for DatabasePtr {}
// SAFETY: VectorScan databases can be shared across threads for scanning
unsafe impl Sync for DatabasePtr {}

/// Stream pointer wrapper
/// 
/// SAFETY INVARIANTS:
/// - Stream pointers are NOT thread-safe (no Sync)
/// - Can be sent between threads but not used concurrently
/// - Must be properly closed before dropping
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct StreamPtr(pub *mut hs_stream_t);

// SAFETY: Streams can be moved between threads but not shared
unsafe impl Send for StreamPtr {}

/// Scratch space pointer wrapper
/// 
/// SAFETY INVARIANTS:
/// - Scratch spaces are thread-local (not Send or Sync)
/// - Each thread must have its own scratch space
/// - Scratch must be allocated for specific database
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct ScratchPtr(pub *mut hs_scratch_t);

// SAFETY: Scratch spaces can be sent between threads
// This is safe because the scratch itself doesn't contain thread-local state
unsafe impl Send for ScratchPtr {}

/// Chimera database pointer wrapper
#[cfg(has_chimera)]
#[repr(transparent)]
pub struct ChimeraDatabasePtr(pub *mut ch_database_t);

#[cfg(has_chimera)]
unsafe impl Send for ChimeraDatabasePtr {}
#[cfg(has_chimera)]
unsafe impl Sync for ChimeraDatabasePtr {}

/// Chimera scratch pointer wrapper
#[cfg(has_chimera)]
#[repr(transparent)]
pub struct ChimeraScratchPtr(pub *mut ch_scratch_t);

#[cfg(has_chimera)]
unsafe impl Send for ChimeraScratchPtr {}

/// Compile flags
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Flags: u32 {
        const CASELESS = HS_FLAG_CASELESS;
        const DOTALL = HS_FLAG_DOTALL;
        const MULTILINE = HS_FLAG_MULTILINE;
        const SINGLEMATCH = HS_FLAG_SINGLEMATCH;
        const ALLOWEMPTY = HS_FLAG_ALLOWEMPTY;
        const UTF8 = HS_FLAG_UTF8;
        const UCP = HS_FLAG_UCP;
        const PREFILTER = HS_FLAG_PREFILTER;
        const SOM_LEFTMOST = HS_FLAG_SOM_LEFTMOST;
    }
}

/// Compile mode flags
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Mode: u32 {
        const BLOCK = HS_MODE_BLOCK;
        const STREAM = HS_MODE_STREAM;
        const VECTORED = HS_MODE_VECTORED;
        const SOM_HORIZON_LARGE = HS_MODE_SOM_HORIZON_LARGE;
        const SOM_HORIZON_MEDIUM = HS_MODE_SOM_HORIZON_MEDIUM;
        const SOM_HORIZON_SMALL = HS_MODE_SOM_HORIZON_SMALL;
    }
}

/// Platform information
#[derive(Debug, Clone)]
pub struct Platform {
    pub tune: u32,
    pub cpu_features: u64,
}

impl Platform {
    /// Detect current platform automatically
    pub fn detect() -> Result<Self, String> {
        let mut info = hs_platform_info {
            tune: 0,
            cpu_features: 0,
            reserved1: 0,
            reserved2: 0,
        };
        
        let ret = unsafe {
            hs_populate_platform(&mut info)
        };
        
        if ret != HS_SUCCESS as i32 {
            return Err(format!("Failed to detect platform: {}", ret));
        }
        
        Ok(Self {
            tune: info.tune,
            cpu_features: info.cpu_features,
        })
    }
    
    /// Create platform for current CPU (alias for detect)
    pub fn native() -> Self {
        let mut info = hs_platform_info {
            tune: 0,
            cpu_features: 0,
            reserved1: 0,
            reserved2: 0,
        };
        
        unsafe {
            hs_populate_platform(&mut info);
        }
        
        Self {
            tune: info.tune,
            cpu_features: info.cpu_features,
        }
    }
}

/// Extended expression info
#[derive(Debug, Clone, Default)]
pub struct ExpressionExt {
    pub flags: u64,
    pub min_offset: u64,
    pub max_offset: u64,
    pub min_length: u64,
    pub edit_distance: u32,
    pub hamming_distance: u32,
}

impl ExpressionExt {
    pub const FLAG_MIN_OFFSET: u64 = 1 << 0;
    pub const FLAG_MAX_OFFSET: u64 = 1 << 1;
    pub const FLAG_MIN_LENGTH: u64 = 1 << 2;
    pub const FLAG_EDIT_DISTANCE: u64 = 1 << 3;
    pub const FLAG_HAMMING_DISTANCE: u64 = 1 << 4;
}

/// Compile multiple patterns with extended parameters
pub fn compile_extended(
    patterns: &[&str],
    flags: &[Flags],
    ids: &[u32],
    ext: &[ExpressionExt],
    mode: Mode,
    platform: Option<&Platform>,
) -> Result<DatabasePtr, CompileError> {
    assert_eq!(patterns.len(), flags.len());
    assert_eq!(patterns.len(), ids.len());
    assert_eq!(patterns.len(), ext.len());
    
    // SAFETY: Converting Rust strings to C strings
    // - Each CString is heap-allocated and valid until explicitly freed
    // - CString::new ensures no null bytes in patterns
    // - into_raw transfers ownership, we must free later
    let pattern_ptrs: Vec<*const c_char> = patterns
        .iter()
        .map(|p| CString::new(*p).unwrap().into_raw() as *const c_char)
        .collect();
    
    let flag_values: Vec<c_uint> = flags.iter().map(|f| f.bits()).collect();
    
    // SAFETY: Convert to hyperscan ext structs
    // - Each Box::new allocates on heap
    // - Box::into_raw transfers ownership, we must free later
    // - The structs are POD types safe for FFI
    let ext_ptrs: Vec<*const hs_expr_ext_t> = ext
        .iter()
        .map(|e| {
            Box::into_raw(Box::new(hs_expr_ext_t {
                flags: e.flags,
                min_offset: e.min_offset,
                max_offset: e.max_offset,
                min_length: e.min_length,
                edit_distance: e.edit_distance,
                hamming_distance: e.hamming_distance,
            })) as *const hs_expr_ext_t
        })
        .collect();
    
    let mut database: *mut hs_database_t = ptr::null_mut();
    let mut error: *mut hs_compile_error_t = ptr::null_mut();
    
    let platform_ptr = platform
        .map(|p| &hs_platform_info {
            tune: p.tune,
            cpu_features: p.cpu_features,
            reserved1: 0,
            reserved2: 0,
        } as *const _)
        .unwrap_or(ptr::null());
    
    let ret = unsafe {
        // SAFETY: FFI call requirements
        // - All pointers in pattern_ptrs are valid CStrings
        // - All pointers in ext_ptrs are valid heap allocations
        // - flag_values and ids are valid slices with matching length
        // - database and error pointers are stack-allocated
        // - platform_ptr is either null or points to valid stack data
        hs_compile_ext_multi(
            pattern_ptrs.as_ptr(),
            flag_values.as_ptr(),
            ids.as_ptr(),
            ext_ptrs.as_ptr(),
            patterns.len() as c_uint,
            mode.bits(),
            platform_ptr,
            &mut database,
            &mut error,
        )
    };
    
    // SAFETY: Clean up all allocated memory
    // - Each pattern_ptr was created with CString::into_raw
    // - Must use CString::from_raw to properly deallocate
    for ptr in pattern_ptrs {
        unsafe { CString::from_raw(ptr as *mut c_char); }
    }
    // SAFETY: Clean up all allocated ext structs
    // - Each ext_ptr was created with Box::into_raw
    // - Must use Box::from_raw to properly deallocate
    for ptr in ext_ptrs {
        unsafe { Box::from_raw(ptr as *mut hs_expr_ext_t); }
    }
    
    if ret != HS_SUCCESS as i32 {
        let err = unsafe { VectorScan::extract_compile_error(error) };
        return Err(err);
    }
    
    Ok(DatabasePtr(database))
}

/// Compile error information
#[derive(Debug)]
pub struct CompileError {
    pub message: String,
    pub expression: i32,
    pub position: Option<usize>,
}

/// Pattern validation result
pub struct ValidationError {
    pub message: String,
    pub position: Option<usize>,
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Compile error in expression {}: {}", self.expression, self.message)
    }
}

impl std::error::Error for CompileError {}

/// Match callback return value
#[repr(i32)]
pub enum Matching {
    Continue = 0,
    Terminate = 1,
}

/// Match flags
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MatchFlags: u32 {
        /// This match has a valid SOM value
        const SOM_VALID = 1 << 0;
    }
}

/// Capture group information
#[derive(Debug, Clone)]
pub struct CaptureGroup {
    pub active: bool,
    pub from: i64,
    pub to: i64,
}

/// Chimera match event with capture groups
#[cfg(has_chimera)]
pub type ChimeraMatchCallback = Box<dyn FnMut(u32, u64, u64, u32, &[CaptureGroup]) -> Matching + Send>;

/// Type-erased callback
pub type MatchCallback = Box<dyn FnMut(u32, u64, u64) -> Matching + Send>;

/// Memory allocation function type
pub type AllocFunc = unsafe extern "C" fn(size: usize) -> *mut c_void;
/// Memory free function type
pub type FreeFunc = unsafe extern "C" fn(ptr: *mut c_void);

/// Safe wrappers around VectorScan API
pub struct VectorScan;

impl VectorScan {
    /// Set custom memory allocator
    pub fn set_allocator(alloc_fn: AllocFunc, free_fn: FreeFunc) -> Result<(), String> {
        let ret = unsafe {
            hs_set_allocator(Some(alloc_fn), Some(free_fn))
        };
        
        if ret != HS_SUCCESS as i32 {
            return Err(format!("Failed to set allocator: {}", ret));
        }
        
        Ok(())
    }
    
    /// Clear custom memory allocator (use defaults)
    pub fn clear_allocator() -> Result<(), String> {
        let ret = unsafe {
            hs_set_allocator(None, None)
        };
        
        if ret != HS_SUCCESS as i32 {
            return Err(format!("Failed to clear allocator: {}", ret));
        }
        
        Ok(())
    }
    /// Get version string
    pub fn version() -> &'static str {
        unsafe {
            CStr::from_ptr(hs_version())
                .to_str()
                .unwrap_or("unknown")
        }
    }
    
    /// Check if platform is valid
    pub fn valid_platform() -> Result<(), String> {
        let ret = unsafe { hs_valid_platform() };
        if ret == HS_SUCCESS as i32 {
            Ok(())
        } else {
            Err(format!("Platform not supported: {}", ret))
        }
    }
    
    /// Compile a single pattern
    pub fn compile(
        pattern: &str,
        flags: Flags,
        mode: Mode,
        platform: Option<&Platform>,
    ) -> Result<DatabasePtr, CompileError> {
        let pattern_cstr = CString::new(pattern).map_err(|_| CompileError {
            message: "Pattern contains null byte".to_string(),
            expression: 0,
            position: None,
        })?;
        
        let mut database: *mut hs_database_t = ptr::null_mut();
        let mut error: *mut hs_compile_error_t = ptr::null_mut();
        
        let platform_ptr = platform
            .map(|p| &hs_platform_info {
                tune: p.tune,
                cpu_features: p.cpu_features,
                reserved1: 0,
                reserved2: 0,
            } as *const _)
            .unwrap_or(ptr::null());
        
        let ret = unsafe {
            hs_compile(
                pattern_cstr.as_ptr(),
                flags.bits(),
                mode.bits(),
                platform_ptr,
                &mut database,
                &mut error,
            )
        };
        
        if ret != HS_SUCCESS as i32 {
            let err = unsafe { Self::extract_compile_error(error) };
            return Err(err);
        }
        
        Ok(DatabasePtr(database))
    }
    
    /// Compile literal patterns
    pub fn compile_lit_multi(
        literals: &[&[u8]],
        flags: &[Flags],
        ids: &[u32],
        mode: Mode,
        platform: Option<&Platform>,
    ) -> Result<DatabasePtr, CompileError> {
        assert_eq!(literals.len(), flags.len());
        assert_eq!(literals.len(), ids.len());
        
        let literal_ptrs: Vec<*const c_char> = literals
            .iter()
            .map(|lit| lit.as_ptr() as *const c_char)
            .collect();
        
        let literal_lens: Vec<usize> = literals
            .iter()
            .map(|lit| lit.len())
            .collect();
        
        let flag_values: Vec<c_uint> = flags.iter().map(|f| f.bits()).collect();
        
        let mut database: *mut hs_database_t = ptr::null_mut();
        let mut error: *mut hs_compile_error_t = ptr::null_mut();
        
        let platform_ptr = platform
            .map(|p| &hs_platform_info {
                tune: p.tune,
                cpu_features: p.cpu_features,
                reserved1: 0,
                reserved2: 0,
            } as *const _)
            .unwrap_or(ptr::null());
        
        let ret = unsafe {
            hs_compile_lit_multi(
                literal_ptrs.as_ptr(),
                flag_values.as_ptr(),
                ids.as_ptr(),
                literal_lens.as_ptr(),
                literals.len() as c_uint,
                mode.bits(),
                platform_ptr,
                &mut database,
                &mut error,
            )
        };
        
        if ret != HS_SUCCESS as i32 {
            let err = unsafe { Self::extract_compile_error(error) };
            return Err(err);
        }
        
        Ok(DatabasePtr(database))
    }
    
    /// Compile multiple patterns
    pub fn compile_multi(
        patterns: &[&str],
        flags: &[Flags],
        ids: &[u32],
        mode: Mode,
        platform: Option<&Platform>,
    ) -> Result<DatabasePtr, CompileError> {
        assert_eq!(patterns.len(), flags.len());
        assert_eq!(patterns.len(), ids.len());
        
        let pattern_ptrs: Vec<*const c_char> = patterns
            .iter()
            .map(|p| CString::new(*p).unwrap().into_raw() as *const c_char)
            .collect();
        
        let flag_values: Vec<c_uint> = flags.iter().map(|f| f.bits()).collect();
        
        let mut database: *mut hs_database_t = ptr::null_mut();
        let mut error: *mut hs_compile_error_t = ptr::null_mut();
        
        let platform_ptr = platform
            .map(|p| &hs_platform_info {
                tune: p.tune,
                cpu_features: p.cpu_features,
                reserved1: 0,
                reserved2: 0,
            } as *const _)
            .unwrap_or(ptr::null());
        
        let ret = unsafe {
            hs_compile_multi(
                pattern_ptrs.as_ptr(),
                flag_values.as_ptr(),
                ids.as_ptr(),
                patterns.len() as c_uint,
                mode.bits(),
                platform_ptr,
                &mut database,
                &mut error,
            )
        };
        
        // Clean up CStrings
        for ptr in pattern_ptrs {
            unsafe { CString::from_raw(ptr as *mut c_char); }
        }
        
        if ret != HS_SUCCESS as i32 {
            let err = unsafe { Self::extract_compile_error(error) };
            return Err(err);
        }
        
        Ok(DatabasePtr(database))
    }
    
    /// Allocate scratch space
    pub fn alloc_scratch(database: &DatabasePtr) -> Result<ScratchPtr, String> {
        let mut scratch: *mut hs_scratch_t = ptr::null_mut();
        
        let ret = unsafe {
            hs_alloc_scratch(database.0, &mut scratch)
        };
        
        if ret != HS_SUCCESS as i32 {
            return Err(format!("Failed to allocate scratch: {}", ret));
        }
        
        Ok(ScratchPtr(scratch))
    }
    
    /// Get scratch size
    pub fn scratch_size(scratch: &ScratchPtr) -> Result<usize, String> {
        let mut size: usize = 0;
        
        let ret = unsafe {
            hs_scratch_size(scratch.0, &mut size)
        };
        
        if ret != HS_SUCCESS as i32 {
            return Err(format!("Failed to get scratch size: {}", ret));
        }
        
        Ok(size)
    }
    
    /// Reallocate scratch for a database
    pub fn scratch_realloc(scratch: &mut ScratchPtr, database: &DatabasePtr) -> Result<(), String> {
        let ret = unsafe {
            hs_alloc_scratch(database.0, &mut scratch.0)
        };
        
        if ret != HS_SUCCESS as i32 {
            return Err(format!("Failed to reallocate scratch: {}", ret));
        }
        
        Ok(())
    }
    
    /// Clone scratch space
    pub fn clone_scratch(src: &ScratchPtr) -> Result<ScratchPtr, String> {
        let mut scratch: *mut hs_scratch_t = ptr::null_mut();
        
        let ret = unsafe {
            hs_clone_scratch(src.0, &mut scratch)
        };
        
        if ret != HS_SUCCESS as i32 {
            return Err(format!("Failed to clone scratch: {}", ret));
        }
        
        Ok(ScratchPtr(scratch))
    }
    
    /// Scan data in block mode
    pub fn scan<F>(
        database: &DatabasePtr,
        data: &[u8],
        scratch: &mut ScratchPtr,
        mut on_match: F,
    ) -> Result<(), String>
    where
        F: FnMut(u32, u64, u64) -> Matching,
    {
        // SAFETY: Creating context pointer for FFI callback
        // - on_match lives on our stack frame for the entire FFI call
        // - We pass it as *mut c_void but it's actually &mut F
        // - The callback will cast it back to &mut F safely
        let context = &mut on_match as *mut _ as *mut c_void;
        
        let ret = unsafe {
            // SAFETY: FFI call requirements
            // - database.0 is a valid database pointer
            // - data is a valid byte slice with correct length
            // - scratch.0 is a valid scratch pointer for this database
            // - match_handler is a valid C function pointer
            // - context remains valid for the duration of this call
            hs_scan(
                database.0,
                data.as_ptr() as *const c_char,
                data.len() as c_uint,
                0, // flags
                scratch.0,
                Some(match_handler::<F>),
                context,
            )
        };
        
        match ret {
            x if x == HS_SUCCESS as i32 => Ok(()),
            x if x == HS_SCAN_TERMINATED as i32 => Ok(()),
            _ => Err(format!("Scan failed: {}", ret)),
        }
    }
    
    /// Serialize database
    pub fn serialize_database(database: &DatabasePtr) -> Result<Vec<u8>, String> {
        let mut bytes: *mut c_char = ptr::null_mut();
        let mut length: usize = 0;
        
        let ret = unsafe {
            // SAFETY: FFI call requirements
            // - database.0 is a valid database pointer
            // - bytes and length are stack-allocated and their addresses are valid
            // - hs_serialize_database will allocate memory and store pointer in bytes
            hs_serialize_database(
                database.0,
                &mut bytes,
                &mut length,
            )
        };
        
        if ret != HS_SUCCESS as i32 {
            return Err(format!("Failed to serialize database: {}", ret));
        }
        
        // SAFETY: Memory management
        // - bytes now points to memory allocated by VectorScan
        // - length contains the valid size of that allocation
        // - We must copy the data and free the original allocation
        let data = unsafe {
            slice::from_raw_parts(bytes as *const u8, length).to_vec()
        };
        
        // SAFETY: Freeing VectorScan-allocated memory
        // - bytes was allocated by VectorScan's allocator
        // - Must be freed with libc::free (or custom allocator if set)
        // - No other references to this memory exist after copying to Vec
        unsafe {
            libc::free(bytes as *mut c_void);
        }
        
        Ok(data)
    }
    
    /// Deserialize database
    pub fn deserialize_database(data: &[u8]) -> Result<DatabasePtr, String> {
        let mut database: *mut hs_database_t = ptr::null_mut();
        
        let ret = unsafe {
            hs_deserialize_database(
                data.as_ptr() as *const c_char,
                data.len(),
                &mut database,
            )
        };
        
        if ret != HS_SUCCESS as i32 {
            return Err(format!("Failed to deserialize database: {}", ret));
        }
        
        Ok(DatabasePtr(database))
    }
    
    /// Get database size
    pub fn database_size(database: &DatabasePtr) -> Result<usize, String> {
        let mut size: usize = 0;
        
        let ret = unsafe {
            hs_database_size(database.0, &mut size)
        };
        
        if ret != HS_SUCCESS as i32 {
            return Err(format!("Failed to get database size: {}", ret));
        }
        
        Ok(size)
    }
    
    /// Get stream size
    pub fn stream_size(database: &DatabasePtr) -> Result<usize, String> {
        let mut size: usize = 0;
        
        let ret = unsafe {
            hs_stream_size(database.0, &mut size)
        };
        
        if ret != HS_SUCCESS as i32 {
            return Err(format!("Failed to get stream size: {}", ret));
        }
        
        Ok(size)
    }
    
    /// Get database info
    pub fn database_info(database: &DatabasePtr) -> Result<String, String> {
        let mut info: *mut c_char = ptr::null_mut();
        
        let ret = unsafe {
            hs_database_info(database.0, &mut info)
        };
        
        if ret != HS_SUCCESS as i32 {
            return Err(format!("Failed to get database info: {}", ret));
        }
        
        let info_str = unsafe {
            CStr::from_ptr(info).to_string_lossy().to_string()
        };
        
        unsafe {
            libc::free(info as *mut c_void);
        }
        
        Ok(info_str)
    }
    
    /// Open a stream
    pub fn open_stream(database: &DatabasePtr) -> Result<StreamPtr, String> {
        let mut stream: *mut hs_stream_t = ptr::null_mut();
        
        let ret = unsafe {
            hs_open_stream(database.0, 0, &mut stream)
        };
        
        if ret != HS_SUCCESS as i32 {
            return Err(format!("Failed to open stream: {}", ret));
        }
        
        Ok(StreamPtr(stream))
    }
    
    /// Scan stream
    pub fn scan_stream<F>(
        stream: &mut StreamPtr,
        data: &[u8],
        scratch: &mut ScratchPtr,
        mut on_match: F,
    ) -> Result<(), String>
    where
        F: FnMut(u32, u64, u64) -> Matching,
    {
        let context = &mut on_match as *mut _ as *mut c_void;
        
        let ret = unsafe {
            hs_scan_stream(
                stream.0,
                data.as_ptr() as *const c_char,
                data.len() as c_uint,
                0, // flags
                scratch.0,
                Some(match_handler::<F>),
                context,
            )
        };
        
        match ret {
            x if x == HS_SUCCESS as i32 => Ok(()),
            x if x == HS_SCAN_TERMINATED as i32 => Ok(()),
            _ => Err(format!("Stream scan failed: {}", ret)),
        }
    }
    
    /// Close stream
    pub fn close_stream<F>(
        stream: StreamPtr,
        scratch: &mut ScratchPtr,
        mut on_match: F,
    ) -> Result<(), String>
    where
        F: FnMut(u32, u64, u64) -> Matching,
    {
        let context = &mut on_match as *mut _ as *mut c_void;
        
        let ret = unsafe {
            hs_close_stream(
                stream.0,
                scratch.0,
                Some(match_handler::<F>),
                context,
            )
        };
        
        match ret {
            x if x == HS_SUCCESS as i32 => Ok(()),
            x if x == HS_SCAN_TERMINATED as i32 => Ok(()),
            _ => Err(format!("Failed to close stream: {}", ret)),
        }
    }
    
    /// Free database
    pub fn free_database(database: DatabasePtr) {
        unsafe {
            hs_free_database(database.0);
        }
    }
    
    /// Free scratch
    pub fn free_scratch(scratch: ScratchPtr) {
        unsafe {
            hs_free_scratch(scratch.0);
        }
    }
    
    /// Validate a pattern expression
    pub fn validate_expression(pattern: &str, flags: Flags) -> Result<(), ValidationError> {
        let mut info: *mut hs_expr_info_t = ptr::null_mut();
        let mut error: *mut hs_compile_error_t = ptr::null_mut();
        
        let pattern_cstr = CString::new(pattern).map_err(|_| ValidationError {
            message: "Pattern contains null byte".to_string(),
            position: None,
        })?;
        
        let ret = unsafe {
            hs_expression_info(
                pattern_cstr.as_ptr(),
                flags.bits(),
                &mut info,
                &mut error,
            )
        };
        
        if ret != HS_SUCCESS as i32 {
            let err = unsafe { 
                let msg = CStr::from_ptr((*error).message)
                    .to_string_lossy()
                    .to_string();
                // Try to extract position from error message
                let position = Self::extract_position_from_message(&msg);
                hs_free_compile_error(error);
                ValidationError {
                    message: msg,
                    position,
                }
            };
            return Err(err);
        }
        
        // Free the info structure
        unsafe {
            libc::free(info as *mut c_void);
        }
        
        Ok(())
    }
    
    /// Extract position from error message (e.g., "Parse error at position 10: ...")
    fn extract_position_from_message(msg: &str) -> Option<usize> {
        if let Some(pos_str) = msg.find("position ") {
            let start = pos_str + 9;
            let end = msg[start..].find(|c: char| !c.is_numeric())
                .map(|i| start + i)
                .unwrap_or(msg.len());
            msg[start..end].parse().ok()
        } else {
            None
        }
    }
    
    /// Extract compile error details
    unsafe fn extract_compile_error(error: *mut hs_compile_error_t) -> CompileError {
        let message = CStr::from_ptr((*error).message)
            .to_string_lossy()
            .to_string();
        let position = Self::extract_position_from_message(&message);
        
        let err = CompileError {
            message,
            expression: (*error).expression,
            position,
        };
        hs_free_compile_error(error);
        err
    }
}

/// Chimera API wrapper
#[cfg(has_chimera)]
pub struct Chimera;

#[cfg(has_chimera)]
impl Chimera {
    /// Compile a Chimera pattern with capture group support
    pub fn compile(
        pattern: &str,
        flags: Flags,
        mode: Mode,
        platform: Option<&Platform>,
    ) -> Result<ChimeraDatabasePtr, CompileError> {
        let pattern_cstr = CString::new(pattern).map_err(|_| CompileError {
            message: "Pattern contains null byte".to_string(),
            expression: 0,
            position: None,
        })?;
        
        let mut database: *mut ch_database_t = ptr::null_mut();
        let mut error: *mut ch_compile_error_t = ptr::null_mut();
        
        let platform_ptr = platform
            .map(|p| &hs_platform_info {
                tune: p.tune,
                cpu_features: p.cpu_features,
                reserved1: 0,
                reserved2: 0,
            } as *const _)
            .unwrap_or(ptr::null());
        
        let ret = unsafe {
            ch_compile(
                pattern_cstr.as_ptr(),
                flags.bits(),
                mode.bits(),
                platform_ptr,
                &mut database,
                &mut error,
            )
        };
        
        if ret != CH_SUCCESS as i32 {
            let err = unsafe { Self::extract_compile_error(error) };
            return Err(err);
        }
        
        Ok(ChimeraDatabasePtr(database))
    }
    
    /// Compile multiple Chimera patterns
    pub fn compile_multi(
        patterns: &[&str],
        flags: &[Flags],
        ids: &[u32],
        mode: Mode,
        platform: Option<&Platform>,
    ) -> Result<ChimeraDatabasePtr, CompileError> {
        assert_eq!(patterns.len(), flags.len());
        assert_eq!(patterns.len(), ids.len());
        
        let pattern_ptrs: Vec<*const c_char> = patterns
            .iter()
            .map(|p| CString::new(*p).unwrap().into_raw() as *const c_char)
            .collect();
        
        let flag_values: Vec<c_uint> = flags.iter().map(|f| f.bits()).collect();
        
        let mut database: *mut ch_database_t = ptr::null_mut();
        let mut error: *mut ch_compile_error_t = ptr::null_mut();
        
        let platform_ptr = platform
            .map(|p| &hs_platform_info {
                tune: p.tune,
                cpu_features: p.cpu_features,
                reserved1: 0,
                reserved2: 0,
            } as *const _)
            .unwrap_or(ptr::null());
        
        let ret = unsafe {
            ch_compile_multi(
                pattern_ptrs.as_ptr(),
                flag_values.as_ptr(),
                ids.as_ptr(),
                patterns.len() as c_uint,
                mode.bits(),
                platform_ptr,
                &mut database,
                &mut error,
            )
        };
        
        // Clean up CStrings
        for ptr in pattern_ptrs {
            unsafe { CString::from_raw(ptr as *mut c_char); }
        }
        
        if ret != CH_SUCCESS as i32 {
            let err = unsafe { Self::extract_compile_error(error) };
            return Err(err);
        }
        
        Ok(ChimeraDatabasePtr(database))
    }
    
    /// Allocate scratch for Chimera
    pub fn alloc_scratch(database: &ChimeraDatabasePtr) -> Result<ChimeraScratchPtr, String> {
        let mut scratch: *mut ch_scratch_t = ptr::null_mut();
        
        let ret = unsafe {
            ch_alloc_scratch(database.0, &mut scratch)
        };
        
        if ret != CH_SUCCESS as i32 {
            return Err(format!("Failed to allocate Chimera scratch: {}", ret));
        }
        
        Ok(ChimeraScratchPtr(scratch))
    }
    
    /// Scan with Chimera (captures supported)
    pub fn scan<F>(
        database: &ChimeraDatabasePtr,
        data: &[u8],
        scratch: &mut ChimeraScratchPtr,
        mut on_match: F,
    ) -> Result<(), String>
    where
        F: FnMut(u32, u64, u64, u32, &[CaptureGroup]) -> Matching,
    {
        let context = &mut on_match as *mut _ as *mut c_void;
        
        let ret = unsafe {
            ch_scan(
                database.0,
                data.as_ptr() as *const c_char,
                data.len() as c_uint,
                0, // flags
                scratch.0,
                None, // error event handler
                Some(chimera_match_handler::<F>),
                context,
            )
        };
        
        match ret {
            x if x == CH_SUCCESS as i32 => Ok(()),
            x if x == CH_SCAN_TERMINATED as i32 => Ok(()),
            _ => Err(format!("Chimera scan failed: {}", ret)),
        }
    }
    
    /// Free Chimera database
    pub fn free_database(database: ChimeraDatabasePtr) {
        unsafe {
            ch_free_database(database.0);
        }
    }
    
    /// Free Chimera scratch
    pub fn free_scratch(scratch: ChimeraScratchPtr) {
        unsafe {
            ch_free_scratch(scratch.0);
        }
    }
    
    /// Extract compile error
    unsafe fn extract_compile_error(error: *mut ch_compile_error_t) -> CompileError {
        let message = CStr::from_ptr((*error).message)
            .to_string_lossy()
            .to_string();
        let position = VectorScan::extract_position_from_message(&message);
        
        let err = CompileError {
            message,
            expression: (*error).expression,
            position,
        };
        ch_free_compile_error(error);
        err
    }
}

/// Chimera match handler
/// 
/// SAFETY: FFI Callback Safety for Chimera with capture groups
/// - Context pointer validity: Guaranteed by caller's lifetime
/// - No unwinding: Callback cannot panic across FFI boundary
/// - Capture group handling: Groups pointer is valid for callback duration
/// - Memory safety: All accessed memory is valid for callback duration
#[cfg(has_chimera)]
extern "C" fn chimera_match_handler<F>(
    id: c_uint,
    from: c_ulonglong,
    to: c_ulonglong,
    _flags: c_uint,
    captured: c_uint,
    groups: *const ch_capture_t,
    context: *mut c_void,
) -> c_int
where
    F: FnMut(u32, u64, u64, u32, &[CaptureGroup]) -> Matching,
{
    // SAFETY: context points to a valid F that outlives this callback
    let callback = unsafe { &mut *(context as *mut F) };
    
    // SAFETY: Convert capture groups from C representation
    // - groups pointer is valid if captured > 0
    // - groups points to an array of captured elements
    // - The array is valid for the duration of this callback
    let capture_groups = if captured > 0 && !groups.is_null() {
        unsafe {
            let groups_slice = slice::from_raw_parts(groups, captured as usize);
            groups_slice.iter().map(|g| CaptureGroup {
                active: g.flags & CH_CAPTURE_FLAG_ACTIVE != 0,
                from: g.from as i64,
                to: g.to as i64,
            }).collect::<Vec<_>>()
        }
    } else {
        Vec::new()
    };
    
    match callback(id, from, to, captured, &capture_groups) {
        Matching::Continue => 0,
        Matching::Terminate => 1,
    }
}

/// Match event handler with SOM support
/// 
/// SAFETY: FFI Callback Safety
/// - Context pointer validity: Guaranteed by caller's lifetime
/// - No unwinding: Callback cannot panic across FFI boundary
/// - Memory safety: All accessed memory is valid for callback duration
/// - Thread safety: Callback may be called from any thread
extern "C" fn match_handler_som<F>(
    id: c_uint,
    from: c_ulonglong,
    to: c_ulonglong,
    flags: c_uint,
    context: *mut c_void,
) -> c_int
where
    F: FnMut(u32, u64, u64, u32) -> Matching,
{
    // SAFETY: context points to a valid F that outlives this callback
    // The callback was passed as &mut F where F lives on the caller's stack
    let callback = unsafe { &mut *(context as *mut F) };
    match callback(id, from, to, flags) {
        Matching::Continue => 0,
        Matching::Terminate => 1,
    }
}

/// Match event handler
/// 
/// SAFETY: FFI Callback Safety
/// - Context pointer validity: Guaranteed by caller's lifetime
/// - No unwinding: Callback cannot panic across FFI boundary
/// - Memory safety: All accessed memory is valid for callback duration
/// - Thread safety: Callback may be called from any thread
extern "C" fn match_handler<F>(
    id: c_uint,
    from: c_ulonglong,
    to: c_ulonglong,
    _flags: c_uint,
    context: *mut c_void,
) -> c_int
where
    F: FnMut(u32, u64, u64) -> Matching,
{
    // SAFETY: context points to a valid F that outlives this callback
    // The callback was passed as &mut F where F lives on the caller's stack
    let callback = unsafe { &mut *(context as *mut F) };
    match callback(id, from, to) {
        Matching::Continue => 0,
        Matching::Terminate => 1,
    }
}

// Ensure bitflags is available
pub use bitflags;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_version() {
        let version = VectorScan::version();
        assert!(!version.is_empty());
        println!("VectorScan version: {}", version);
    }
    
    #[test]
    fn test_platform() {
        assert!(VectorScan::valid_platform().is_ok());
    }
}

#[cfg(all(test, miri))]
mod miri_tests {
    use super::*;
    
    #[test]
    fn test_database_ptr_safety() {
        // Test that DatabasePtr properly manages memory
        let pattern = "test";
        let db = VectorScan::compile(pattern, Flags::empty(), Mode::BLOCK, None).unwrap();
        
        // DatabasePtr should be Send + Sync
        std::thread::spawn(move || {
            let _db2 = db;
        }).join().unwrap();
    }
    
    #[test]
    fn test_scratch_ptr_safety() {
        // Test scratch allocation and deallocation
        let pattern = "test";
        let db = VectorScan::compile(pattern, Flags::empty(), Mode::BLOCK, None).unwrap();
        let scratch = VectorScan::alloc_scratch(&db).unwrap();
        
        // Scratch should be Send
        std::thread::spawn(move || {
            let _scratch2 = scratch;
        }).join().unwrap();
    }
    
    #[test]
    fn test_callback_pointer_safety() {
        // Test that callbacks don't cause UB
        let pattern = "test";
        let db = VectorScan::compile(pattern, Flags::empty(), Mode::BLOCK, None).unwrap();
        let mut scratch = VectorScan::alloc_scratch(&db).unwrap();
        
        let mut called = false;
        VectorScan::scan(&db, b"test", &mut scratch, |_, _, _| {
            called = true;
            Matching::Continue
        }).unwrap();
        
        assert!(called);
    }
    
    #[test]
    fn test_cstring_safety() {
        // Test CString conversions
        let patterns = vec!["test1", "test2"];
        let pattern_ptrs: Vec<*const c_char> = patterns
            .iter()
            .map(|p| CString::new(*p).unwrap().into_raw() as *const c_char)
            .collect();
        
        // Clean up properly
        for ptr in pattern_ptrs {
            unsafe { CString::from_raw(ptr as *mut c_char); }
        }
    }
    
    #[test]
    fn test_slice_from_raw_parts_safety() {
        // Test that slice creation is safe
        let data = vec![1u8, 2, 3, 4, 5];
        let ptr = data.as_ptr();
        let len = data.len();
        
        let slice = unsafe {
            slice::from_raw_parts(ptr, len)
        };
        
        assert_eq!(slice, &data[..]);
    }
}
