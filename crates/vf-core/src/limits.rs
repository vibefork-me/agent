//! Upper bounds for data that comes from the network. Everything from the network is hostile: a
//! response larger than its bound is an error, never unbounded memory growth.

/// JSON from an API: GitHub, model providers, model lists.
pub const API_RESPONSE: usize = 16 * 1024 * 1024;

/// A project archive downloaded from GitHub (the unpacked size has its own bound).
pub const PROJECT_ARCHIVE: usize = 500 * 1024 * 1024;

/// One file of an agent release or of the editor bundle.
pub const RELEASE_FILE: usize = 512 * 1024 * 1024;

/// Most files listed for the editor's file tree; the rest are left out and the list says so.
pub const PROJECT_FILES: usize = 20_000;

/// Deepest folder nesting listed for the editor's file tree; deeper files are left out.
pub const PROJECT_FILE_DEPTH: usize = 32;

/// Longest line of a running program's output sent to the editor, in bytes; the rest is cut.
pub const PROGRAM_LINE: usize = 4 * 1024;

/// Most output of one run of a program sent to the editor, in bytes; later lines are dropped.
pub const PROGRAM_OUTPUT: usize = 1024 * 1024;

/// Initial capacity reserved for a body whose size is declared; the rest grows on demand.
const RESERVE_CAP: usize = 1024 * 1024;

/// A body larger than its bound.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the response is larger than {limit} bytes")]
pub struct TooLarge {
    /// The bound that was exceeded.
    pub limit: usize,
}

/// Collects a body chunk by chunk and stops at `limit` bytes.
#[derive(Debug)]
pub struct LimitedBody {
    buf: Vec<u8>,
    limit: usize,
}

impl LimitedBody {
    /// Starts a body of at most `limit` bytes. A `declared` length (`Content-Length`) over the
    /// bound fails at once; within the bound it only sizes the first allocation.
    pub fn new(limit: usize, declared: Option<u64>) -> Result<Self, TooLarge> {
        let declared = declared.map(|d| usize::try_from(d).unwrap_or(usize::MAX));
        if declared.is_some_and(|d| d > limit) {
            return Err(TooLarge { limit });
        }
        let reserve = declared.unwrap_or(0).min(RESERVE_CAP);
        Ok(Self {
            buf: Vec::with_capacity(reserve),
            limit,
        })
    }

    /// Appends a chunk; fails without appending if the body would exceed the bound.
    pub fn push(&mut self, chunk: &[u8]) -> Result<(), TooLarge> {
        match self.buf.len().checked_add(chunk.len()) {
            Some(len) if len <= self.limit => {
                self.buf.extend_from_slice(chunk);
                Ok(())
            }
            _ => Err(TooLarge { limit: self.limit }),
        }
    }

    /// The collected bytes.
    pub fn into_vec(self) -> Vec<u8> {
        self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_chunks_up_to_the_bound() {
        let mut body = LimitedBody::new(5, None).unwrap();
        body.push(b"ab").unwrap();
        body.push(b"cde").unwrap();
        assert_eq!(body.into_vec(), b"abcde");
    }

    #[test]
    fn rejects_a_chunk_past_the_bound_and_keeps_what_fits() {
        let mut body = LimitedBody::new(4, None).unwrap();
        body.push(b"abc").unwrap();
        assert_eq!(body.push(b"de"), Err(TooLarge { limit: 4 }));
        assert_eq!(body.into_vec(), b"abc");
    }

    #[test]
    fn rejects_a_declared_length_over_the_bound() {
        assert_eq!(
            LimitedBody::new(10, Some(11)).unwrap_err(),
            TooLarge { limit: 10 }
        );
        assert!(LimitedBody::new(10, Some(10)).is_ok());
        assert!(LimitedBody::new(10, Some(u64::MAX)).is_err());
    }

    #[test]
    fn a_huge_declared_length_does_not_reserve_it() {
        let body = LimitedBody::new(usize::MAX, Some(u64::MAX >> 1)).unwrap();
        assert!(body.buf.capacity() <= RESERVE_CAP);
    }
}
