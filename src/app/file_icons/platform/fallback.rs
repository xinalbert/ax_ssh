use super::*;

#[derive(Default)]
pub(in crate::app::file_icons) struct Resolver {
    _private: (),
}

impl IconResolver for Resolver {
    fn cache_identity(&self) -> CacheIdentity {
        CacheIdentity::new("unsupported-platform-v1")
    }

    fn resolve(&self, _key: &FileIconKey) -> Option<FileIcon> {
        None
    }
}
