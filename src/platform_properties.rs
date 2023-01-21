use alloc::sync::Arc;

/// Properties of paths of a specific platform
pub trait PlatformProperties: core::fmt::Debug {
    /// Returns a list of separators used on the platform.
    ///
    /// The first in the list is assumed to be the preferred one.
    fn separators(&self) -> &[char];

    /// Returns the name of the root object in the path, if there is one
    fn root_name<'a>(&'a self, path: &'a str) -> Option<(&'a str, &'a str)>;
}

/// Type-erased platform properties
#[derive(Debug, Clone)]
pub struct PlatformPropertiesOpaque {
    inner: Arc<dyn PlatformProperties>,
}

impl<T> From<T> for PlatformPropertiesOpaque
where
    T: PlatformProperties + 'static,
{
    fn from(properties: T) -> PlatformPropertiesOpaque {
        PlatformPropertiesOpaque {
            inner: Arc::new(properties),
        }
    }
}

impl core::ops::Deref for PlatformPropertiesOpaque {
    type Target = dyn PlatformProperties + 'static;

    fn deref(&self) -> &(dyn PlatformProperties + 'static) {
        &*self.inner
    }
}

/// Properties of Windows paths
#[derive(Clone, Copy, Debug, Default)]
pub struct Windows {}

/// Properties of Unix-style paths
#[derive(Clone, Copy, Debug, Default)]
pub struct Unix {}

impl Windows {
    fn get_drive_name(path: &str) -> Option<&str> {
        for (idx, c) in path.chars().take(2).enumerate() {
            match idx {
                0 => {
                    if !('A'..='Z').contains(&c.to_ascii_uppercase()) {
                        return None;
                    }
                }
                1 => return if c == ':' { Some(&path[0..2]) } else { None },
                _ => unreachable!("get_drive_name iterated further than two characters"),
            }
        }
        None
    }
}

impl PlatformProperties for Windows {
    fn separators(&self) -> &[char] {
        ['\\', '/'].as_ref()
    }

    fn root_name<'a>(&'a self, path: &'a str) -> Option<(&str, &str)> {
        let drive_name = Self::get_drive_name(path);
        drive_name.map(|drive_name| (drive_name, &path[drive_name.len()..]))
    }
}

impl PlatformProperties for Unix {
    fn separators(&self) -> &[char] {
        ['/'].as_ref()
    }

    fn root_name<'a>(&'a self, path: &'a str) -> Option<(&str, &str)> {
        if path.starts_with("/") {
            Some((&"", path))
        } else {
            None
        }
    }
}
