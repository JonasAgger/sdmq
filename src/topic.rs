use core::fmt::Debug;

#[derive(Clone, Copy)]
pub struct Topic<'a> {
    inner: &'a str,
}

impl<'a> Topic<'a> {
    pub(crate) fn parse(bytes: &'a [u8]) -> Self {
        // SAFETY:
        // This can only be constructed from a known valid UTF8 string.
        Self {
            inner: unsafe { core::str::from_utf8_unchecked(bytes) },
        }
    }

    pub fn main(&self) -> &str {
        let Some((right, _)) = self.inner.split_once('.') else {
            return self.inner;
        };

        right
    }

    pub fn get(&self, idx: usize) -> &'a str {
        self.inner.split('.').skip(idx).next().unwrap_or("")
    }

    #[cfg(feature = "std")]
    pub fn to_string(&self) -> String {
        self.inner.to_owned()
    }
}

impl<'a> Debug for Topic<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.inner)
    }
}
