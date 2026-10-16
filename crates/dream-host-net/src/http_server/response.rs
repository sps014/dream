use super::state::{self, Body, error, lock};
use crate::socket_operations::{Failure, Output};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{
    Response, StatusCode,
    header::{HeaderName, HeaderValue},
};
use serde_json::Value;

pub(super) fn simple(status: u16, bytes: &[u8]) -> Response<Body> {
    let mut response = Response::new(Full::new(Bytes::copy_from_slice(bytes)).boxed_unsync());
    *response.status_mut() =
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    response
}
pub(super) fn headers(response: &mut Response<Body>, value: &Value) -> Result<(), Failure> {
    let status = value["status"]
        .as_u64()
        .and_then(|n| u16::try_from(n).ok())
        .and_then(|n| StatusCode::from_u16(n).ok())
        .ok_or_else(|| error("EINVAL", "invalid HTTP status"))?;
    *response.status_mut() = status;
    for pair in value["headers"]
        .as_array()
        .ok_or_else(|| error("EINVAL", "invalid HTTP headers"))?
    {
        let name = pair[0]
            .as_str()
            .ok_or_else(|| error("EINVAL", "invalid HTTP header name"))?;
        let value = pair[1]
            .as_str()
            .ok_or_else(|| error("EINVAL", "invalid HTTP header value"))?;
        response.headers_mut().append(
            HeaderName::from_bytes(name.as_bytes()).map_err(|e| error("EINVAL", e))?,
            HeaderValue::from_str(value).map_err(|e| error("EINVAL", e))?,
        );
    }
    Ok(())
}
pub(super) fn send(id: i32, value: &Value, bytes: Vec<u8>) -> Result<Output, Failure> {
    let request = state::request(id)?;
    let mut response = simple(200, &bytes);
    headers(&mut response, value)?;
    let reply = lock(&request.reply)
        .take()
        .ok_or_else(|| error("ECLOSED", "HTTP response already started"))?;
    reply
        .send(response)
        .map_err(|_| error("ECLOSED", "HTTP client disconnected"))?;
    Ok(Output::bytes(vec![]))
}
