extern crate rustc_serialize;
#[macro_use]
extern crate log;
use rustc_serialize::json::{Json, ToJson, ParserError};
use std::collections::{BTreeMap, HashMap};

/**
 * Enum with possible errors.
 * */
#[derive(Debug)]
pub enum ErrorCode {
    /**
     * Request is not valid JSON.
     * */
    ParseError,
    /**
     * Invalid request, eg. invalid request object, lack of required fields etc.
     * */
    InvalidRequest,
    /**
     * Method with given name is not existing.
     * */
    MethodNotFound,
    /**
     * Requested method is not defined for given set of parameters.
     * */
    InvalidParams,
    /**
     * Internal server error (eg. OOM, cosmic rays, etc.)
     * */
    InternalError,
    /**
     * Custom defined server errors.
     * Error code should be between -32099 and -32000.
     * */
    ServerError(i32, &'static str),
}

/**
 * Handler for processing request.
 * */
pub trait Handler {
    type Context;
    fn handle(&self, reg: &JsonRpcRequest, custom: &Self::Context) -> Result<Json, ErrorJsonRpc>;
}

/**
 * Internal enum used to determine if error was thrown when id was already known or not.
 * */
#[derive(Debug)]
enum InternalErrorCode {
    /**
     * Used when request contains correct id (also None)
     * */
    WithId(ErrorCode, Option<Json>, Option<Json>),
    /**
     * Special case when error is returned before request id could be determined.
     * */
    WithoutId(ErrorCode, Option<Json>),
}

impl InternalErrorCode {
    /**
     * Converts InternalErrorCode to JsonRpcResponse.
     * */
    fn into_response(self) -> JsonRpcResponse {
        let (err, id, data) = match self {
            InternalErrorCode::WithId(err, id, data) => (err, id, data),
            // Convert to Json::Null
            InternalErrorCode::WithoutId(err, data) => (err, Some(Json::Null), data),
        };
        JsonRpcResponse::new_error(err, data, id)
    }
}

// Convinient method for getting integer value for error
impl ErrorCode {
    /**
     * Retrieve error code.
     * */
    fn get_code(&self) -> i32 {
        match *self {
            ErrorCode::ParseError => -32700,
            ErrorCode::InvalidRequest => -32600,
            ErrorCode::MethodNotFound => -32601,
            ErrorCode::InvalidParams => -32602,
            ErrorCode::InternalError => -32603,
            ErrorCode::ServerError(x, _) => x,
        }
    }

    /**
     * Get short description for error.
     * */
    fn get_desc(&self) -> &'static str {
        match *self {
            ErrorCode::ParseError => "Parse error",
            ErrorCode::InvalidRequest => "Invalid Request",
            ErrorCode::MethodNotFound => "Method not found",
            ErrorCode::InvalidParams => "Invalid params",
            ErrorCode::InternalError => "Internal error",
            ErrorCode::ServerError(_, s) => s,
        }
    }

    /**
     * Sanity check if requested custom error code is in valid range.
     * Well-Defined errors are always valid.
     * */
    fn is_valid(&self) -> bool {
        match *self {
            // Error code is only valid within that range
            ErrorCode::ServerError(-32099...-32000, _) => true,
            // All remaining ServerError enums are invalid
            ErrorCode::ServerError(_, _) => false,
            // All predefined codes are valid
            _ => true,
        }
    }
}

/**
 * Object describing client request.
 * */
pub struct JsonRpcRequest<'a> {
    /**
     * Name of remote procedure to call.
     * */
    pub method: &'a str,

    /**
     * Parameters to method. Only Object (request by position) or Array (request by name).
     * */
    pub params: Option<&'a Json>,

    /**
     * Request id from client. If None client send notification and don't want any response.
     * Only OBJECT type is prohibited.
     * This should remain provate field.
     * */
    id: Option<&'a Json>,
}

/**
 * Describe Error response
 * */
#[derive(Debug)]
pub struct ErrorJsonRpc {
    /**
     * Error code
     * */
    error: ErrorCode,

    /**
     * Extra information and details
     * */
    data: Option<Json>,
}

impl ErrorJsonRpc {
    /**
     * Make new Error response instance without additional data.
     * */
    pub fn new(err: ErrorCode) -> ErrorJsonRpc {
        ErrorJsonRpc {
            error: err,
            data: None,
        }
    }

    /**
     * Make new error response instance with additiobnal data field
     * */
    pub fn new_data(err: ErrorCode, data: Json) -> ErrorJsonRpc {
        ErrorJsonRpc {
            error: err,
            data: Some(data),
        }
    }

    /**
     * Get code for error
     * */
    pub fn get_code(&self) -> i32 {
        self.error.get_code()
    }

    /**
     * Get short description message for error
     * */
    pub fn get_message(&self) -> &str {
        self.error.get_desc()
    }

    /**
     * Get additional data for error.
     * */
    pub fn get_data(&self) -> Option<&Json> {
        self.data.as_ref()
    }
}


impl ToJson for ErrorJsonRpc {
    /**
     * Convert ErrorJsonRpc to Json
     * */
    fn to_json(&self) -> Json {
        let mut d = BTreeMap::new();
        d.insert("code".to_owned(), self.get_code().to_json());
        d.insert("message".to_owned(), self.get_message().to_json());
        if let Some(data) = self.get_data() {
            d.insert("data".to_owned(), data.clone());
        }
        Json::Object(d)
    }
}

/**
 * Describe response from server to client.
 * */
pub struct JsonRpcResponse {
    /**
     * Result of method invocation. None if error occured.
     * */
    result: Option<Json>,

    /**
     * Descibe error. This field is None on success.
     * */
    error: Option<ErrorJsonRpc>,

    /**
     * Response id. Exactly match id from request. Value is None only for notification.
     * */
    id: Option<Json>,
}

impl JsonRpcResponse {
    /**
     * Build response with error
     * */
    fn new_error(err: ErrorCode, data: Option<Json>, id: Option<Json>) -> JsonRpcResponse {
        let error = if err.is_valid() {
            err
        } else {
            ErrorCode::InternalError
        };
        JsonRpcResponse {
            result: None,
            error: match data {
                Some(data) => Some(ErrorJsonRpc::new_data(error, data)),
                None => Some(ErrorJsonRpc::new(error)),
            },
            id: id,
        }
    }

    /**
     * Build response with result
     * */
    fn new_result(req: &JsonRpcRequest, data: Json) -> JsonRpcResponse {
        JsonRpcResponse {
            result: Some(data),
            error: None,
            id: req.id.cloned(),
        }
    }
}

impl ToJson for JsonRpcResponse {
    /**
     * Convert JsonRpcResponse to Json
     * */
    fn to_json(&self) -> Json {
        if self.id == None {
            return Json::Null;
        }
        let mut d = BTreeMap::new();
        d.insert("jsonrpc".to_owned(), "2.0".to_owned().to_json());
        if let Some(ref result) = self.result {
            d.insert("result".to_owned(), result.clone());
        }
        if let Some(ref error) = self.error {
            d.insert("error".to_owned(), error.to_json());
        }
        if let Some(ref id) = self.id {
            d.insert("id".to_owned(), id.clone());
        }
        Json::Object(d)
    }
}

/**
 * JSON-RPC processing unit.
 * */
pub struct JsonRpcServer<H: Handler + 'static> {
    handler: H,
}

pub type HashMapWithMethods = HashMap<String, Box<Fn(&JsonRpcRequest) -> Result<Json, ErrorJsonRpc> + 'static + Sync + Send>>;
impl Handler for HashMapWithMethods {
    type Context = ();
    fn handle(&self, req: &JsonRpcRequest, _: &Self::Context) -> Result<Json, ErrorJsonRpc> {
        self.get(req.method)
            .ok_or_else(|| {
                error!("Requested method '{}' not found!", req.method);
                ErrorJsonRpc::new(ErrorCode::MethodNotFound)
            })
            .and_then(|s| s(&req))
    }
}

impl JsonRpcServer<HashMapWithMethods> {
    /**
     * Create new default instance of JsonRpcServer.
     * */
    pub fn new() -> JsonRpcServer<HashMapWithMethods> {
        JsonRpcServer { handler: Default::default() }
    }
}

impl From<ParserError> for InternalErrorCode {
    fn from(_: ParserError) -> InternalErrorCode {
        InternalErrorCode::WithoutId(ErrorCode::ParseError, None)
    }
}
impl <H: Handler> JsonRpcServer<H> where H::Context: Default {
    /// Specialized implementation for context implementing default trait
    pub fn handle_request(&self, req: &str) -> Option<String> {
        self.handle_request_context(req, &Default::default())
    }
}
impl <H: Handler> JsonRpcServer<H> {
    /**
     * Create instance of JsonRpcServer with custom handler
     * */
    pub fn new_handler(h: H) -> JsonRpcServer<H> {
        JsonRpcServer { handler: h }
    }

    fn _handle_single(&self,
                      req: &rustc_serialize::json::Object,
                      custom: &H::Context)
                      -> Result<JsonRpcResponse, InternalErrorCode> {

        // Ensure field jsonrpc exist and contains string "2.0"
        if !req.get("jsonrpc")
               .and_then(|o| o.as_string())
               .map_or(false, |s| s == "2.0") {
            return Err(InternalErrorCode::WithoutId(ErrorCode::InvalidRequest, None));
        }

        // try parse ID and then pass it to error message
        let request_id = req.get("id");

        if let Some(&Json::Object(_)) = request_id {
            return Err(InternalErrorCode::WithoutId(ErrorCode::InvalidRequest, None));
        }

        // At this point we know assigned id
        let request_method = if let Some(s) = req.get("method").and_then(|m| m.as_string()) {
            s
        } else {
            return Err(InternalErrorCode::WithoutId(ErrorCode::InvalidRequest, None));
        };

        let request_params = match req.get("params") {
            Some(json) => match *json {
                Json::Array(_) | Json::Object(_) => Some(json),
                Json::Null => None,
                _ => return Err(InternalErrorCode::WithoutId(ErrorCode::InvalidRequest, None)),
            },
            None => None,
        };

        // From now request is considered as VALID and code should use WithId
        let request = JsonRpcRequest {
            method: request_method,
            params: request_params,
            id: request_id,
        };

        self.handler
            .handle(&request, custom)
            .map(|s| JsonRpcResponse::new_result(&request, s))
            .map_err(move |e| {
                InternalErrorCode::WithId(e.error, request.id.cloned(), e.data)
            })
    }

    fn _handle_multiple(&self,
                        array: &rustc_serialize::json::Array,
                        custom: &H::Context)
                        -> Result<Option<Json>, InternalErrorCode> {
        if array.is_empty() {
            return Err(InternalErrorCode::WithoutId(ErrorCode::InvalidRequest, None));
        }

        // Convert to vector (required by json api)
        let response_vector: Vec<_> = array.iter()
                .filter_map(|request| {
                    info!("Processing {}", request);
                    let response = request.as_object()
                            // Convert None to error
                            .ok_or_else(|| InternalErrorCode::WithoutId(ErrorCode::InvalidRequest, None))
                            // Invoke remote procedure
                            .and_then(|o|self._handle_single(o, custom))
                            // Convert any error to Json
                            .unwrap_or_else(|e|e.into_response());
                            // Skip notifications in response
                            if response.id == None {
                                None
                            } else {
                                Some(response)
                            }
                }).collect();

        // All notifications nothing to respond
        if response_vector.is_empty() {
            Ok(None)
        } else {
            Ok(Some(response_vector.to_json()))
        }
    }

    fn _handle_request(&self,
                       request: &str,
                       custom: &H::Context) -> Result<Option<Json>, InternalErrorCode> {
        let request_json = try!(Json::from_str(&request));

        // for now only plain object support
        match request_json {
            Json::Object(ref s) => self._handle_single(s, custom).map(|m| Some(m.to_json())),
            Json::Array(ref a) => self._handle_multiple(a, custom),
            _ => Err(InternalErrorCode::WithoutId(ErrorCode::InvalidRequest, None)),
        }
    }

    pub fn handle_request_context(&self, request: &str, custom: &H::Context) -> Option<String> {
        let result = self._handle_request(&request, custom);
        match result {
            Ok(Some(ref resp)) if *resp != Json::Null => Some(resp.to_json().to_string()),
            // Notification (but got some data?), no returned response anyway
            Ok(Some(ref some)) => {
                warn!("Co to jest?: {:?}", some);
                None
            }
            Ok(_) => None,
            Err(err) => {
                let response = err.into_response().to_json();
                if response == Json::Null {
                    println!("Empty");
                    None
                } else {
                    Some(response.to_string())
                }
            }
        }
    }

    /**
     * Get handler reference
     * */
    pub fn get_handler(&self) -> &H {
        &self.handler
    }

    /**
     * Get mutable handler reference
     * */
    pub fn get_handler_mut(&mut self) -> &mut H {
        &mut self.handler
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustc_serialize::json::{Json, ToJson};

    // tests from JSON-RPC RFC
    #[test]
    fn test_positional() {
        let mut handler = HashMapWithMethods::new();
        handler.insert("subtract".to_owned(), Box::new(|_| Ok(19.to_json())));
        let server = JsonRpcServer::new_handler(handler);
        let request = "{\"jsonrpc\": \"2.0\", \"method\": \"subtract\", \"params\": [42, 23], \
                       \"id\": 1}";
        let expected_response = Json::from_str("{\"jsonrpc\": \"2.0\", \"result\": 19, \"id\": 1}");
        let response = Json::from_str(&server.handle_request(request).unwrap());
        assert_eq!(expected_response, response);
    }

    #[test]
    fn test_named() {
        let mut handler = HashMapWithMethods::new();
        handler.insert("subtract".to_owned(), Box::new(|_| Ok(19.to_json())));
        let server = JsonRpcServer::new_handler(handler);
        let request = "{\"jsonrpc\": \"2.0\", \"method\": \"subtract\", \"params\": \
                       {\"subtrahend\": 23, \"minuend\": 42}, \"id\": 3}";
        let expected_response = Json::from_str("{\"jsonrpc\": \"2.0\", \"result\": 19, \"id\": 3}");
        let response = Json::from_str(&server.handle_request(request).unwrap());
        assert_eq!(expected_response, response);
    }

    #[test]
    fn test_empty_params() {
        let mut handler = HashMapWithMethods::new();
        handler.insert("simple_method".to_owned(), Box::new(|_| Ok(1.to_json())));
        let server = JsonRpcServer::new_handler(handler);
        let request = "{\"jsonrpc\": \"2.0\", \"method\": \"simple_method\", \"params\": null, \
                       \"id\": 3}";
        let expected_response = Json::from_str("{\"jsonrpc\": \"2.0\", \"result\": 1, \"id\": 3}");
        let response = Json::from_str(&server.handle_request(request).unwrap());
        assert_eq!(expected_response, response);

        let request = "{\"jsonrpc\": \"2.0\", \"method\": \"simple_method\", \"params\": null, \
                       \"id\": 3}";
        let response = Json::from_str(&server.handle_request(request).unwrap());
        assert_eq!(expected_response, response);
    }

    #[test]
    fn test_notification() {
        // --> {"jsonrpc": "2.0", "method": "update", "params": [1,2,3,4,5]}
        // --> {"jsonrpc": "2.0", "method": "foobar"}
        let mut handler = HashMapWithMethods::new();
        handler.insert("update".to_owned(), Box::new(|_| Ok(Json::Null)));
        handler.insert("foobar".to_owned(), Box::new(|_| Ok(Json::Null)));
        let server = JsonRpcServer::new_handler(handler);
        let response = server.handle_request("{\"jsonrpc\": \"2.0\", \"method\": \"update\", \
                                              \"params\": [1,2,3,4,5]}");
        assert_eq!(None, response);
        assert_eq!(None,
                   server.handle_request("{\"jsonrpc\": \"2.0\", \"method\": \"foobar\"}"));
    }

    #[test]
    fn test_non_existing_method() {
        // --> {"jsonrpc": "2.0", "method": "foobar", "id": "1"}
        // <-- {"jsonrpc": "2.0", "error": {"code": -32601, "message": "Method not
        // found"}, "id": "1"}
        let server = JsonRpcServer::new();
        let request = "{\"jsonrpc\": \"2.0\", \"method\": \"foobar\",
            \"id\": \"1\"}";
        let expected_response = Json::from_str("{\"jsonrpc\": \"2.0\",
            \"error\": \
                                                {\"code\": -32601, \"message\": \"Method not \
                                                found\"},
            \"id\": \"1\"}");
        let response = Json::from_str(&server.handle_request(request).unwrap());
        assert_eq!(expected_response, response);
    }
    #[test]
    fn test_call_invalid_json() {
        // --> {"jsonrpc": "2.0", "method": "foobar, "params": "bar", "baz]
        // <-- {"jsonrpc": "2.0", "error": {"code": -32700, "message": "Parse error"},
        // "id": null}
        let server = JsonRpcServer::new();
        let request = "{\"jsonrpc\": \"2.0\", \"method\": \"foobar, \"params\": \"bar\", \"baz]";
        let expected_response = Json::from_str("{\"jsonrpc\": \"2.0\", \"error\": {\"code\": \
                                                -32700, \"message\": \"Parse error\"}, \"id\": \
                                                null}");
        let response = Json::from_str(&server.handle_request(request).unwrap());
        assert_eq!(expected_response, response);
    }

    #[test]
    fn test_call_invalid_request() {
        // --> {"jsonrpc": "2.0", "method": 1, "params": "bar"}
        // <-- {"jsonrpc": "2.0", "error": {"code": -32600, "message": "Invalid
        // Request"}, "id": null}
        let server = JsonRpcServer::new();
        let request = "{\"jsonrpc\": \"2.0\", \"method\": 1, \"params\": \"bar\"}";
        let expected_response = Json::from_str("{\"jsonrpc\": \"2.0\", \"error\": {\"code\": \
                                                -32600, \"message\": \"Invalid Request\"}, \
                                                \"id\": null}");
        let response = Json::from_str(&server.handle_request(request).unwrap());
        assert_eq!(expected_response, response);
    }

    #[test]
    fn test_call_batch_invalid_json() {
        let request = "[
            {\"jsonrpc\": \"2.0\", \"method\": \"sum\", \"params\": \
                       [1,2,4], \"id\": \"1\"},
            {\"jsonrpc\": \"2.0\", \"method\"
        \
                       ]";
        let server = JsonRpcServer::new();
        let expected_response = Json::from_str("{\"jsonrpc\": \"2.0\", \"error\": {\"code\": \
                                                -32700, \"message\": \"Parse error\"}, \"id\": \
                                                null}");
        let response = Json::from_str(&server.handle_request(request).unwrap());
        assert_eq!(expected_response, response);
    }

    #[test]
    fn test_call_with_empty_array() {
        let request = "[]";
        let expected_response = Json::from_str("{\"jsonrpc\": \"2.0\", \"error\": {\"code\": \
                                                -32600, \"message\": \"Invalid Request\"}, \
                                                \"id\": null}");
        let server = JsonRpcServer::new();
        let response = Json::from_str(&server.handle_request(request).unwrap());
        assert_eq!(expected_response, response);
    }

    #[test]
    fn test_call_with_invalid_batch_not_empty() {
        let request = "[1]";
        let expected_response = Json::from_str("[{\"jsonrpc\": \"2.0\", \"error\": {\"code\": \
                                                -32600, \"message\": \"Invalid Request\"}, \
                                                \"id\": null}]");
        let server = JsonRpcServer::new();
        let response = Json::from_str(&server.handle_request(request).unwrap());
        assert_eq!(expected_response, response);
    }

    #[test]
    fn test_call_with_invalid_batch() {
        let request = "[1,2,3]";
        let expected_response = Json::from_str("[
            {\"jsonrpc\": \"2.0\", \"error\": \
                                                {\"code\": -32600, \"message\": \"Invalid \
                                                Request\"}, \"id\": null},
            \
                                                {\"jsonrpc\": \"2.0\", \"error\": {\"code\": \
                                                -32600, \"message\": \"Invalid Request\"}, \
                                                \"id\": null},
            {\"jsonrpc\": \
                                                \"2.0\", \"error\": {\"code\": -32600, \
                                                \"message\": \"Invalid Request\"}, \"id\": null}
            \
                                                ]");
        let server = JsonRpcServer::new();
        let response = Json::from_str(&server.handle_request(request).unwrap());
        assert_eq!(expected_response, response);
    }

    #[test]
    fn test_call_batch() {
        let request = "[
        {\"jsonrpc\": \"2.0\", \"method\": \"sum\", \"params\": [1,2,4], \
                       \"id\": \"1\"},
        {\"jsonrpc\": \"2.0\", \"method\": \
                       \"notify_hello\", \"params\": [7]},
        {\"jsonrpc\": \"2.0\", \
                       \"method\": \"subtract\", \"params\": [42,23], \"id\": \"2\"},
        \
                       {\"foo\": \"boo\"},
        {\"jsonrpc\": \"2.0\", \"method\": \
                       \"foo.get\", \"params\": {\"name\": \"myself\"}, \"id\": \"5\"},
        \
                       {\"jsonrpc\": \"2.0\", \"method\": \"get_data\", \"id\": \"9\"}
        ]";

        let expected_response = Json::from_str("[
        {\"jsonrpc\": \"2.0\", \"result\": 7, \
                                                \"id\": \"1\"},
        {\"jsonrpc\": \"2.0\", \
                                                \"result\": 19, \"id\": \"2\"},
        \
                                                {\"jsonrpc\": \"2.0\", \"error\": {\"code\": \
                                                -32600, \"message\": \"Invalid Request\"}, \
                                                \"id\": null},
        {\"jsonrpc\": \"2.0\", \
                                                \"error\": {\"code\": -32601, \"message\": \
                                                \"Method not found\"}, \"id\": \"5\"},
        \
                                                {\"jsonrpc\": \"2.0\", \"result\": [\"hello\", \
                                                5], \"id\": \"9\"}
        ]");

        let mut server = JsonRpcServer::new();
        {
            let mut handler = server.get_handler_mut();
            handler.insert("sum".to_owned(), Box::new(|_| Ok(7.to_json())));
            handler.insert("notify_hello".to_owned(), Box::new(|_| Ok(Json::Null)));
            handler.insert("subtract".to_owned(), Box::new(|_| Ok(19.to_json())));
            handler.insert("get_data".to_owned(),
                           Box::new(|_| Ok(vec!["hello".to_json(), 5.to_json()].to_json())));
        }
        let response = Json::from_str(&server.handle_request(request).unwrap());
        assert_eq!(expected_response, response);
    }

    #[test]
    fn test_call_batch_all_notifications() {
        let request = "[
        {\"jsonrpc\": \"2.0\", \"method\": \"notify_sum\", \"params\": \
                       [1,2,4]},
        {\"jsonrpc\": \"2.0\", \"method\": \"notify_hello\", \
                       \"params\": [7]}
        ]";
        let mut handler = HashMapWithMethods::new();
        handler.insert("notify_sum".to_owned(), Box::new(|_| Ok(Json::Null)));
        handler.insert("notify_hello".to_owned(), Box::new(|_| Ok(Json::Null)));
        let server = JsonRpcServer::new_handler(handler);
        let response = server.handle_request(request);
        assert_eq!(None, response);
    }
}
