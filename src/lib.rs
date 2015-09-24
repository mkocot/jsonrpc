extern crate rustc_serialize;
#[macro_use]
extern crate log;
use rustc_serialize::json::{Json, ToJson};
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
     * Custom defined server errors. Error code should be between -32099 and -32000.
     * */
    ServerError(i32, &'static str),
}

pub trait Handler {
    fn handle(&self, reg: &JsonRpcRequest) -> Result<Json, ErrorCode>;
}
/**
 * Internal enum used to determine if error was thrown when id was already known or not.
 * */
#[derive(Debug)]
enum InternalErrorCode {
    /**
     * Used when request contains correct id (also None)
     * */
    WithId(ErrorCode, Option<Json>),
    /**
     * Special case when error is returned before request id could be determined.
     * */
    WithoutId(ErrorCode)
}

impl InternalErrorCode {
    /**
     * Converts InternalErrorCode to JsonRpcResponse.
     * */
    fn as_response(self) -> JsonRpcResponse {
        let (err, id) = match self {
            InternalErrorCode::WithId(err, id) => (err,id),
            //Convert to Json::Null 
            InternalErrorCode::WithoutId(err) => (err, Some(Json::Null))
        };
        JsonRpcResponse::new_error(err, None, id)
    }
}

//Convinient method for getting integer value for error
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
            ErrorCode::ServerError(x, _) => x
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
            ErrorCode::ServerError(_, s) => s
        }
    }

    /**
     * Sanity check if requested custom error code is in valid range.
     * Well-Defined errors are always valid.
     * */
    fn is_valid(&self) -> bool {
        match *self {
            //Error code is only valid within that range
            ErrorCode::ServerError(-32099...-32000, _) => true,
            //All remaining ServerError enums are invalid
            ErrorCode::ServerError(_,_) => false,
            //All predefined codes are valid
            _ => true
        }
    }
}

/**
 * Object describing client request.
 * */
pub struct JsonRpcRequest {
    /**
     * Name of remote procedure to call.
     * */
    pub method: String,

    /**
     * Parameters to method. Only Object (request by position) or Array (request by name).
     * */
    pub params: Option<Json>,

    /**
     * Request id from client. If None client send notification and don't want any response.
     * Only OBJECT type is prohibited.
     * This should remain provate field.
     * */
    id: Option<Json>
}


struct ErrorJsonRpc {
    code: i32,
    message: String,
    data: Option<Json>
}

impl ToJson for ErrorJsonRpc {
    fn to_json(&self) -> Json {
        let mut d = BTreeMap::new();
        d.insert("code".to_string(), self.code.to_json());
        d.insert("message".to_string(), self.message.to_json());
        if let Some(ref data) = self.data {
            d.insert("data".to_string(), data.clone());
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
     * Response id. Exactly match id from request. Value is never None.
     * */
    id: Option<Json>
}

impl JsonRpcResponse {
    fn new_error(err: ErrorCode, data: Option<Json>, id: Option<Json>)
        -> JsonRpcResponse {
        let error = if err.is_valid() { err } else { ErrorCode::InternalError };
        JsonRpcResponse {
            //jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(ErrorJsonRpc {
                code: error.get_code(),
                message: error.get_desc().to_string(),
                data: data
            }),
            id: id
        }
    }
    fn new_result(req: &JsonRpcRequest, data: Json) -> JsonRpcResponse {
        JsonRpcResponse {
            result: Some(data),
            error: None,
            id: req.id.clone()
        }
    }
}

impl ToJson for JsonRpcResponse {
    //Simple serialization
    fn to_json(&self) -> Json {
        if self.id == None {
            return ().to_json();
        }
        let mut d = BTreeMap::new();
        d.insert("jsonrpc".to_string(), "2.0".to_string().to_json());
        if let Some(ref result) = self.result {
            d.insert("result".to_string(), result.clone());
        }
        if let Some(ref error) = self.error {
            d.insert("error".to_string(), error.to_json());
        }
        if let Some(ref id) = self.id {
            d.insert("id".to_string(), id.clone());
        }
        Json::Object(d)
    }
}
/**
 * JSON-RPC processing unit.
 * */
pub struct JsonRpcServer {
    /**
     * Map with closures and functions assigned with names.
     * */
    methods: HashMap<String, Box<Fn(&JsonRpcRequest) -> Result<Json, ErrorCode> + 'static + Sync + Send>>,
}

impl Handler for JsonRpcServer {
    fn handle(&self, req: &JsonRpcRequest) -> Result<Json, ErrorCode> {
        self.methods.get(&req.method).ok_or_else(||{
            error!("Requested method '{}' not found!", req.method);
            ErrorCode::MethodNotFound
        }).and_then(|s|s(&req))
    }
}

impl JsonRpcServer {
    /**
     * Create new instance of JsonRpcServer.
     * */
    pub fn new() -> JsonRpcServer {
        JsonRpcServer {
            methods: HashMap::new(),
        }
    }

    /**
     * Adds method with name to server.
     * */
    pub fn register_str<F>(&mut self, name: &str, f: F)
        where F: Fn(&JsonRpcRequest) -> Result<Json, ErrorCode> + 'static + Sync + Send {
        self.methods.insert(name.to_string(), Box::new(f));
    }

    /**
     * Remove method from known names.
     * */
    pub fn unregister_str(&mut self, name: &str) {
        self.methods.remove(name);
    }

    fn _handle_single_with_id<H: Handler>(&self, req: &rustc_serialize::json::Object, request_id: &Option<Json>, h: &H)
        -> Result<JsonRpcResponse, InternalErrorCode> {
        let request_method = match req.get("method").and_then(|m|m.as_string()) {
            Some(s) => s.to_string(),
            _ => return Err(InternalErrorCode::WithoutId(ErrorCode::InvalidRequest))
        };

        let request_params = match req.get("params") {
            Some(json) => match json {
                &Json::Array(_) => Some(json.clone()),
                &Json::Object(_) => Some(json.clone()),
                _ => return Err(InternalErrorCode::WithoutId(ErrorCode::InvalidRequest)),
            },
            None => None
        };

        //From now request is considered as VALID and code should use WithId
        let request = JsonRpcRequest {
            //Required
            method: request_method,
            //Optional
            params: request_params,
            //Optional
            id: request_id.clone()
        };
        h.handle(&request).map(|s| JsonRpcResponse::new_result(&request, s))
        .map_err(move |e| InternalErrorCode::WithId(e, request.id))
    }

    fn _handle_single<H: Handler>(&self, req: &rustc_serialize::json::Object, h: &H)
        -> Result<JsonRpcResponse, InternalErrorCode> {
        // Ensure field jsonrpc exist and contains string "2.0"
        if !req.get("jsonrpc").and_then(|o|o.as_string())
            .and_then(|s|Some(s == "2.0")).unwrap_or(false) {
            return Err(InternalErrorCode::WithoutId(ErrorCode::InvalidRequest))
        }

        //try parse ID and then pass it to error message
        let request_id = match req.get("id") {
            Some(json) => match *json {
                //Allow only primitives
                //We are using WithoutId becaouse we can't trust this request object
                Json::Object(_) => 
                    return Err(InternalErrorCode::WithoutId(ErrorCode::InvalidRequest)),
                _ => Some(json.clone())
            },
            None => None
        };

        //At this point we know assigned id
        self._handle_single_with_id(req, &request_id, h)
    }

    fn _handle_multiple<H: Handler>(&self, array: &rustc_serialize::json::Array, h: &H)
        -> Result<Option<Json>, InternalErrorCode> {
        let mut response_vector = Vec::new();
        if array.is_empty() {
            return Err(InternalErrorCode::WithoutId(ErrorCode::InvalidRequest));
        }

        for request in array {
            info!("Processing {}", request);
            let response = request
                .as_object()
                //Convert None to error
                .ok_or(InternalErrorCode::WithoutId(ErrorCode::InvalidRequest))
                //Invoke remote procedure
                .and_then(|o|self._handle_single(o, h))
                //Convert any error to Json
                .unwrap_or_else(|e|e.as_response());

            //Skip notifications in response
            if response.id != None {
                response_vector.push(response);
            }
        }

        //All notifications nothing to respond
        if response_vector.is_empty() {
            Ok(None)
        } else {
            Ok(Some(response_vector.to_json()))
        }
    }

    fn _handle_request<H: Handler>(&self, request: &str, h: &H)
        -> Result<Option<Json>, InternalErrorCode> {
        let request_json = match Json::from_str(&request) {
            Ok(o) => o,
            Err(_) => return Err(InternalErrorCode::WithoutId(ErrorCode::ParseError))
        };

        //for now only plain object support
        match request_json {
            Json::Object(ref s) => self._handle_single(s, h).map(|m|Some(m.to_json())),
            Json::Array(ref a) => self._handle_multiple(a, h),
            _ => return Err(InternalErrorCode::WithoutId(ErrorCode::InvalidRequest))
        }
    }
    //request: Raw json
    //return: Raw json
    pub fn handle_request(&self, request: String) -> String {
        self.handle_custom(self, &request)
    }

    pub fn handle_custom<H: Handler + 'static>(&self, h: &H, request: &String) -> String {
        let result = self._handle_request(&request, h);
        match result {
            Ok(Some(ref resp)) if *resp != Json::Null => resp.to_json().to_string(),
            //Notification, no response
            Ok(Some(ref some)) => {
                warn!("Co to jest?: {:?}", some);
                "".to_string()
            },
            Ok(_) => "".to_string(),
            Err(err) => {
                let response = err.as_response().to_json();
                if response == Json::Null {
                    println!("Empty");
                    "".to_string()
                } else {
                    response.to_string()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustc_serialize::json::{Json, ToJson};

    //tests from JSON-RPC RFC
    #[test]
    fn test_positional() {
        let mut server = JsonRpcServer::new();
        server.register_str("subtract", |_| Ok(19.to_json()));
        let request = "{\"jsonrpc\": \"2.0\", \"method\": \"subtract\",
                        \"params\": [42, 23], \"id\": 1}";
        let expected_response = Json::from_str("{\"jsonrpc\": \"2.0\",
                                               \"result\": 19, \"id\": 1}");
        let response = Json::from_str(&server.handle_request(request.to_string()));
        println!("Expected: {:?}", expected_response);
        println!("Received: {:?}", response);
        assert!(expected_response == response);
    }

    #[test]
    fn test_named() {
        let mut server = JsonRpcServer::new();
        server.register_str("subtract", |_| Ok(19.to_json()));
        let request = "{\"jsonrpc\": \"2.0\", \"method\": \"subtract\",
            \"params\": {\"subtrahend\": 23, \"minuend\": 42}, \"id\": 3}";
        let expected_response = Json::from_str("{\"jsonrpc\": \"2.0\",
                                               \"result\": 19, \"id\": 3}");
        let response = Json::from_str(&server.handle_request(request.to_string()));
        println!("Expected: {:?}", expected_response);
        println!("Received: {:?}", response);
        assert!(expected_response == response);
    }
    #[test]
    fn test_notification() {
        //--> {"jsonrpc": "2.0", "method": "update", "params": [1,2,3,4,5]}
        //--> {"jsonrpc": "2.0", "method": "foobar"}
        let mut server = JsonRpcServer::new();
        server.register_str("update", |_| Ok(Json::Null));
        server.register_str("foobar", |_| Ok(Json::Null));
        let response = server.handle_request("{\"jsonrpc\": \"2.0\",                                                         \"method\": \"update\",
                                        \"params\": [1,2,3,4,5]}".to_string());
        println!("Received: {:?}", response);
        assert!("".to_string() == response);
        assert!("".to_string() == server.handle_request("{\"jsonrpc\": \"2.0\", \"method\": \"foobar\"}".to_string()));
    }

    #[test]
    fn test_non_existing_method() {
    //--> {"jsonrpc": "2.0", "method": "foobar", "id": "1"}
    //<-- {"jsonrpc": "2.0", "error": {"code": -32601, "message": "Method not found"}, "id": "1"}
        let server = JsonRpcServer::new();
        let request = "{\"jsonrpc\": \"2.0\", \"method\": \"foobar\",
            \"id\": \"1\"}";
        let expected_response = Json::from_str("{\"jsonrpc\": \"2.0\",
            \"error\": {\"code\": -32601, \"message\": \"Method not found\"},
            \"id\": \"1\"}");
        let response = Json::from_str(&server.handle_request(request.to_string()));
        println!("Expected: {:?}", expected_response);
        println!("Received: {:?}", response);
        assert!(expected_response == response);
    }
    #[test]
    fn test_call_invalid_json() {
    //--> {"jsonrpc": "2.0", "method": "foobar, "params": "bar", "baz]
    //<-- {"jsonrpc": "2.0", "error": {"code": -32700, "message": "Parse error"}, "id": null}
        let server = JsonRpcServer::new();
        let request = "{\"jsonrpc\": \"2.0\", \"method\": \"foobar, \"params\": \"bar\", \"baz]";
        let expected_response = Json::from_str("{\"jsonrpc\": \"2.0\", \"error\": {\"code\": -32700, \"message\": \"Parse error\"}, \"id\": null}");
        let response = Json::from_str(&server.handle_request(request.to_string()));
        println!("Expected: {:?}", expected_response);
        println!("Received: {:?}", response);
        assert!(expected_response == response);
    }

    #[test]
    fn test_call_invalid_request() {
    //--> {"jsonrpc": "2.0", "method": 1, "params": "bar"}
    //<-- {"jsonrpc": "2.0", "error": {"code": -32600, "message": "Invalid Request"}, "id": null}
        let server = JsonRpcServer::new();
        let request = "{\"jsonrpc\": \"2.0\", \"method\": 1, \"params\": \"bar\"}";
        let expected_response = Json::from_str("{\"jsonrpc\": \"2.0\", \"error\": {\"code\": -32600, \"message\": \"Invalid Request\"}, \"id\": null}");
        let response = Json::from_str(&server.handle_request(request.to_string()));
        println!("Expected: {:?}", expected_response);
        println!("Received: {:?}", response);
        assert!(expected_response == response);
    }

    #[test]
    fn test_call_batch_invalid_json() {
        let request ="[
            {\"jsonrpc\": \"2.0\", \"method\": \"sum\", \"params\": [1,2,4], \"id\": \"1\"},
            {\"jsonrpc\": \"2.0\", \"method\"
        ]";
        let server = JsonRpcServer::new();
        let expected_response = Json::from_str("{\"jsonrpc\": \"2.0\", \"error\": {\"code\": -32700, \"message\": \"Parse error\"}, \"id\": null}");
        let response = Json::from_str(&server.handle_request(request.to_string()));
        println!("Expected: {:?}", expected_response);
        println!("Received: {:?}", response);
        assert!(expected_response == response);
    }

    #[test]
    fn test_call_with_empty_array() {
        let request = "[]";
        let expected_response = Json::from_str("{\"jsonrpc\": \"2.0\", \"error\": {\"code\": -32600, \"message\": \"Invalid Request\"}, \"id\": null}");
        let server = JsonRpcServer::new();
        let response = Json::from_str(&server.handle_request(request.to_string()));
        println!("Expected: {:?}", expected_response);
        println!("Received: {:?}", response);
        assert!(expected_response == response);
    }

    #[test]
    fn test_call_with_invalid_batch_not_empty() {
        let request = "[1]";
        let expected_response = Json::from_str("[
    {\"jsonrpc\": \"2.0\", \"error\": {\"code\": -32600, \"message\": \"Invalid Request\"}, \"id\": null}
]");
        let server = JsonRpcServer::new();
        let response = Json::from_str(&server.handle_request(request.to_string()));
        println!("Expected: {:?}", expected_response);
        println!("Received: {:?}", response);
        assert!(expected_response == response);
    }

    #[test]
    fn test_call_with_invalid_batch() {
        let request = "[1,2,3]";
        let expected_response = Json::from_str("[
            {\"jsonrpc\": \"2.0\", \"error\": {\"code\": -32600, \"message\": \"Invalid Request\"}, \"id\": null},
            {\"jsonrpc\": \"2.0\", \"error\": {\"code\": -32600, \"message\": \"Invalid Request\"}, \"id\": null},
            {\"jsonrpc\": \"2.0\", \"error\": {\"code\": -32600, \"message\": \"Invalid Request\"}, \"id\": null}
            ]");
        let server = JsonRpcServer::new();
        let response = Json::from_str(&server.handle_request(request.to_string()));
        println!("Expected: {:?}", expected_response);
        println!("Received: {:?}", response);
        assert!(expected_response == response);
    }

    #[test]
    fn test_call_batch() {
        let request = "[
        {\"jsonrpc\": \"2.0\", \"method\": \"sum\", \"params\": [1,2,4], \"id\": \"1\"},
        {\"jsonrpc\": \"2.0\", \"method\": \"notify_hello\", \"params\": [7]},
        {\"jsonrpc\": \"2.0\", \"method\": \"subtract\", \"params\": [42,23], \"id\": \"2\"},
        {\"foo\": \"boo\"},
        {\"jsonrpc\": \"2.0\", \"method\": \"foo.get\", \"params\": {\"name\": \"myself\"}, \"id\": \"5\"},
        {\"jsonrpc\": \"2.0\", \"method\": \"get_data\", \"id\": \"9\"}
        ]";

        let expected_response = Json::from_str("[
        {\"jsonrpc\": \"2.0\", \"result\": 7, \"id\": \"1\"},
        {\"jsonrpc\": \"2.0\", \"result\": 19, \"id\": \"2\"},
        {\"jsonrpc\": \"2.0\", \"error\": {\"code\": -32600, \"message\": \"Invalid Request\"}, \"id\": null},
        {\"jsonrpc\": \"2.0\", \"error\": {\"code\": -32601, \"message\": \"Method not found\"}, \"id\": \"5\"},
        {\"jsonrpc\": \"2.0\", \"result\": [\"hello\", 5], \"id\": \"9\"}
        ]");

        let mut server = JsonRpcServer::new();
        server.register_str("sum", |_| Ok(7.to_json()));
        server.register_str("notify_hello", |_| Ok(Json::Null));
        server.register_str("subtract", |_| Ok(19.to_json()));
        server.register_str("get_data", |_| Ok(vec!["hello".to_json(), 5.to_json()].to_json()));
        let response = Json::from_str(&server.handle_request(request.to_string()));
        println!("Expected: {:?}", expected_response);
        println!("Received: {:?}", response);
        assert!(expected_response == response);
    }

    #[test]
    fn test_call_batch_all_notifications() {
        let request = "[
        {\"jsonrpc\": \"2.0\", \"method\": \"notify_sum\", \"params\": [1,2,4]},
        {\"jsonrpc\": \"2.0\", \"method\": \"notify_hello\", \"params\": [7]}
        ]";
        let mut server = JsonRpcServer::new();
        server.register_str("notify_sum", |_| Ok(Json::Null));
        server.register_str("notify_hello", |_| Ok(Json::Null));
        let response = server.handle_request(request.to_string());
        println!("Expected: ");
        println!("Received: {}", response);
        assert!("" == response);
    }
}
