use std::convert::Infallible;
use hyper::{Method, Request, body::Incoming, Response};
use http_body_util::combinators::BoxBody;
use bytes::Bytes;

async fn route(req: Request<Incoming>) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible> {
    let (parts, body) = req.into_parts();
    let method = parts.method;
    let path = parts.uri.path();

    // ...
    Ok(Response::new(http_body_util::Empty::new().map_err(|never| match never {}).boxed()))
}

fn main() {}
