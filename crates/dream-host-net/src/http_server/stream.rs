use super::{
    response,
    state::{self, Body, Request, error, lock},
};
use crate::socket_operations::{Failure, Output};
use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::{BodyExt, StreamBody};
use hyper::{
    Response,
    body::{Body as HttpBody, Frame, SizeHint},
};
use std::{
    convert::Infallible,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

struct GuardBody {
    inner: Body,
    request: Arc<Request>,
}
impl HttpBody for GuardBody {
    type Data = Bytes;
    type Error = Infallible;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        ctx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        Pin::new(&mut self.inner).poll_frame(ctx)
    }
    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }
    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}
impl Drop for GuardBody {
    fn drop(&mut self) {
        self.request.disconnected.send_replace(true);
    }
}
pub(super) fn guard(response: Response<Body>, request: Arc<Request>) -> Response<Body> {
    if request.upgraded.load(std::sync::atomic::Ordering::Acquire) {
        return response;
    }
    response.map(|inner| GuardBody { inner, request }.boxed_unsync())
}
pub(super) fn begin(id: i32, args: &[u8]) -> Result<Output, Failure> {
    let request = state::request(id)?;
    let (sender, receiver) = mpsc::channel(8);
    let body = StreamBody::new(
        ReceiverStream::new(receiver).map(|bytes| Ok::<_, Infallible>(Frame::data(bytes))),
    )
    .boxed_unsync();
    let mut response = Response::new(body);
    response::head(&mut response, &mut dream_host_abi::wire::Reader::new(args))?;
    let reply = lock(&request.reply)
        .take()
        .ok_or_else(|| error("ECLOSED", "HTTP response already started"))?;
    *lock(&request.stream) = Some(sender);
    reply
        .send(response)
        .map_err(|_| error("ECLOSED", "HTTP client disconnected"))?;
    Ok(Output::bytes(vec![]))
}
pub(super) async fn write(id: i32, bytes: Vec<u8>) -> Result<Output, Failure> {
    let request = state::request(id)?;
    let sender = lock(&request.stream)
        .clone()
        .ok_or_else(|| error("ECLOSED", "HTTP stream is closed"))?;
    sender
        .send(Bytes::from(bytes))
        .await
        .map_err(|_| error("ECLOSED", "HTTP client disconnected"))?;
    Ok(Output::bytes(vec![]))
}
pub(super) fn end(id: i32) -> Result<Output, Failure> {
    lock(&state::request(id)?.stream).take();
    Ok(Output::bytes(vec![]))
}
