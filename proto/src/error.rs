use super::*;

use std::error;
use std::fmt;

/* TODO: "Get Error" endpoint... something like
#[derive(Serialize, Deserialize, Schema)]
pub struct ErrorQuery {
    pub seq_no: u32,
    pub key: Key,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct LastErrorResponse(pub Option<DispatchError>);

#[derive(Serialize, Deserialize, Schema)]
pub enum DispatchError {
    NonexistentEndpoint
}
*/

#[derive(Debug, Serialize, Deserialize, Schema)]
pub struct RequestError {}

impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "driver could not honor request, check error endpoint for details"
        )
    }
}

impl error::Error for RequestError {}
