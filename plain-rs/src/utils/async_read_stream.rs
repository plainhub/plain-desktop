//! Adapter from `tokio::io::AsyncRead` to `futures_core::Stream<Item = Result<Bytes, io::Error>>`.
//!
//! Ported from plain-nas `src/async_read_stream.rs`, which replaced its
//! single use of `tokio_util::io::ReaderStream` to avoid pulling in
//! `tokio-util` (and its dependency fan-out) for one call site.
//!
//! Behaviour matches `tokio_util::io::ReaderStream::with_capacity(r, 64 KiB)`:
//! read up to 64 KiB per `poll_next`, yield each chunk as a `Bytes`, end
//! with `None` on EOF. Like `ReaderStream`, the read goes **directly into
//! the `BytesMut` that becomes the yielded chunk** (kernel → buffer, no
//! intermediate copy); this is the streaming hot path for `/fs` file
//! serving on weak CPUs.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{BufMut, Bytes, BytesMut};
use futures_core::Stream;
use tokio::io::AsyncRead;

/// Chunk size per `poll_next`.
const CAPACITY: usize = 64 * 1024;

/// Adapter that turns an `AsyncRead` into a `Stream<Item = Result<Bytes, io::Error>>`.
pub struct AsyncReadStream<R> {
    reader: R,
    buf: BytesMut,
}

impl<R: AsyncRead + Unpin> AsyncReadStream<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buf: BytesMut::with_capacity(CAPACITY),
        }
    }
}

impl<R: AsyncRead + Unpin> Stream for AsyncReadStream<R> {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::task::ready;
        let this = self.get_mut();
        if !this.buf.has_remaining_mut() {
            this.buf.reserve(CAPACITY);
        }
        let n = {
            let dst = this.buf.chunk_mut();
            // SAFETY: `UninitSlice` is a transparent wrapper around
            // `[MaybeUninit<u8>]` (same conversion tokio-util uses); we
            // only hand it to `ReadBuf`, which never reads the
            // uninitialized bytes, only writes into them.
            let dst = unsafe { dst.as_uninit_slice_mut() };
            let mut read_buf = tokio::io::ReadBuf::uninit(dst);
            let ptr = read_buf.filled().as_ptr();
            ready!(Pin::new(&mut this.reader).poll_read(cx, &mut read_buf)?);
            // The pointer must not have moved from under us.
            assert_eq!(ptr, read_buf.filled().as_ptr());
            read_buf.filled().len()
        };
        if n == 0 {
            return Poll::Ready(None); // EOF
        }
        // SAFETY: `poll_read` reported exactly `n` bytes of the spare
        // capacity as initialized, so advancing by that amount is sound.
        unsafe { this.buf.advance_mut(n) };
        Poll::Ready(Some(Ok(this.buf.split().freeze())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn streams_then_ends_at_eof() {
        use futures::StreamExt;
        let data = vec![7u8; 200_000]; // > 64 KiB → multiple chunks
        let mut stream = Box::pin(AsyncReadStream::new(Cursor::new(data)));
        let mut total = 0;
        let mut chunks = 0;
        while let Some(res) = stream.as_mut().next().await {
            let bytes = res.unwrap();
            assert!(bytes.len() <= 64 * 1024);
            assert_eq!(bytes[0], 7);
            total += bytes.len();
            chunks += 1;
        }
        assert_eq!(total, 200_000);
        assert!(chunks >= 3);
    }

    #[tokio::test]
    async fn chunk_content_is_byte_exact_across_boundaries() {
        use futures::StreamExt;
        // Position-dependent pattern: any off-by-one copy bug would smear it.
        let data: Vec<u8> = (0..150_000u32).map(|i| (i % 251) as u8).collect();
        let mut stream = Box::pin(AsyncReadStream::new(Cursor::new(data.clone())));
        let mut got = Vec::new();
        while let Some(res) = stream.as_mut().next().await {
            got.extend_from_slice(&res.unwrap());
        }
        assert_eq!(got, data);
    }

    #[tokio::test]
    async fn zero_len_reader_ends_immediately() {
        use futures::StreamExt;
        let mut stream = Box::pin(AsyncReadStream::new(Cursor::new(Vec::<u8>::new())));
        assert!(stream.as_mut().next().await.is_none());
    }
}
