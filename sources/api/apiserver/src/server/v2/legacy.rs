//! Provides legacy-compatible API methods to maintain compatibility with software written before
//! the introduction of settings extensions to Bottlerocket.

use actix_web::web;

pub fn register_legacy_routes(cfg: &mut web::ServiceConfig) {
    // TODO! We should provide a backwards-compatible unversioned API to maintain
    // existing bottlerocket variants

    // Use the implementation in crate::server::v1 to guide the implementation here.
}
