use alloc::sync::Arc;

/// Properties of paths of a specific platform
pub trait PlatformProperties: core::fmt::Debug {
    /// Returns a list of separators used on the platform.
    ///
    /// The first in the list is assumed to be the preferred one.
    fn separators(&self) -> &[char];
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

impl PlatformProperties for Windows {
    fn separators(&self) -> &[char] {
        ['\\', '/'].as_ref()
    }
}

impl PlatformProperties for Unix {
    fn separators(&self) -> &[char] {
        ['/'].as_ref()
    }
}
