use super::{
    request,
    state::{self, error},
};
use crate::socket_operations::{Failure, Output};
use bytes::Bytes;
use futures_util::stream;
use serde_json::json;
use std::convert::Infallible;

pub(super) async fn parse(id: i32) -> Result<Output, Failure> {
    let request = state::request(id)?;
    let headers = request.metadata["headers"]
        .as_array()
        .ok_or_else(|| error("EINVAL", "missing HTTP headers"))?;
    let content_type = headers
        .iter()
        .find(|h| h[0].as_str() == Some("content-type"))
        .and_then(|h| h[1].as_str())
        .unwrap_or("");
    let boundary = multer::parse_boundary(content_type).map_err(|e| error("EINVAL", e))?;
    let bytes = request::body(id).await?;
    let source = stream::once(async move { Ok::<_, Infallible>(Bytes::from(bytes)) });
    let constraints = multer::Constraints::new().size_limit(
        multer::SizeLimit::new()
            .whole_stream(state::MAX_BODY as u64)
            .per_field(state::MAX_BODY as u64),
    );
    let mut multipart = multer::Multipart::with_constraints(source, boundary, constraints);
    let mut fields = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| error("EINVAL", e))?
    {
        let name = field.name().unwrap_or("").to_string();
        let filename = field.file_name().map(str::to_string);
        let content_type = field
            .content_type()
            .map(ToString::to_string)
            .unwrap_or_default();
        let bytes = field.bytes().await.map_err(|e| error("EINVAL", e))?;
        fields.push(json!({"name":name,"filename":filename,"content_type":content_type,"bytes":hex::encode(bytes)}));
    }
    Ok(Output::bytes(json!(fields).to_string().into_bytes()))
}
