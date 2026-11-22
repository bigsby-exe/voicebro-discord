use libloading::{Error, Library};
use std::path::Path;

/// Load a trusted library by path, resolving bundled DLLs beside it on Windows.
///
/// # Safety
/// The caller must trust the library's initializers and validate its ABI.
pub(crate) unsafe fn load(path: &Path) -> Result<Library, Error> {
    #[cfg(windows)]
    {
        use libloading::os::windows::{Library as WindowsLibrary, LOAD_WITH_ALTERED_SEARCH_PATH};
        unsafe { WindowsLibrary::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH) }
            .map(Into::into)
    }
    #[cfg(not(windows))]
    unsafe {
        Library::new(path)
    }
}
