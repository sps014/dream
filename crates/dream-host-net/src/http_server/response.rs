use super::state::{self, Body, error, lock};
use crate::socket_operations::{Failure, Output};
use bytes::Bytes;
use dream_host_abi::wire::Reader;
use http_body_util::{BodyExt, Full};
use hyper::{
    Response, StatusCode,
    header::{HeaderName, HeaderValue},
};

pub(super) fn simple(status: u16, bytes: &[u8]) -> Response<Body> {
    let mut response = Response::new(Full::new(Bytes::copy_from_slice(bytes)).boxed_unsync());
    *response.status_mut() =
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    response
}
/// Applies a response head: `[status][header count][name, value]...`.
pub(super) fn head(response: &mut Response<Body>, reader: &mut Reader<'_>) -> Result<(), Failure> {
    let malformed = |_| error("EINVAL", "malformed HTTP response head");
    *response.status_mut() = u16::try_from(reader.i32().map_err(malformed)?)
        .ok()
        .and_then(|n| StatusCode::from_u16(n).ok())
        .ok_or_else(|| error("EINVAL", "invalid HTTP status"))?;
    let count = reader.i32().map_err(malformed)?;
    for _ in 0..count.max(0) {
        let name = reader.text().map_err(malformed)?;
        let value = reader.text().map_err(malformed)?;
        response.headers_mut().append(
            HeaderName::from_bytes(name.as_bytes()).map_err(|e| error("EINVAL", e))?,
            HeaderValue::from_str(value).map_err(|e| error("EINVAL", e))?,
        );
    }
    reader.finish().map_err(malformed)
}
pub(super) fn send(id: i32, args: &[u8], bytes: Vec<u8>) -> Result<Output, Failure> {
    let request = state::request(id)?;
    let mut response = simple(200, &bytes);
    head(&mut response, &mut Reader::new(args))?;
    let reply = lock(&request.reply)
        .take()
        .ok_or_else(|| error("ECLOSED", "HTTP response already started"))?;
    reply
        .send(response)
        .map_err(|_| error("ECLOSED", "HTTP client disconnected"))?;
    Ok(Output::bytes(vec![]))
}
