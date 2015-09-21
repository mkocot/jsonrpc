extern crate rustc_serialize;
use rustc_serialize::json::{self, Json, ToJson};
use std::collections::{BTreeMap, HashMap};

pub enum ErrorCode {
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    InternalError,
    //from -32000 to -32099
    ServerError(i32, &'static str),
}

//Convinient method for getting integer value for error
impl ErrorCode {
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

pub struct JsonRpcRequest {
    //exactly "2.0"
    //jsonrpc: String, no point of sticking it there
    pub method: String,
    //by-position: ARRAY
    //by-name: Object
    pub params: Option<Json>,

    //HIDDEN
    //String, Number or NULL
    id: Option<Json>
}

pub struct ErrorJsonRpc {
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

pub struct JsonRpcResponse {
    //exactly "2.0" (no point of sticking it there)
    //jsonrpc: String,
    //Only if success
    result: Option<Json>,
    //Only if failure
    error: Option<ErrorJsonRpc>,
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
            //Id is required. Convert None -> Json:Null 
            id: id.or(Some(Json::Null))
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
// Server
pub struct JsonRpcServer {
    //type RpcFunction = Fn(Json) -> Result<Json, ErrorCode> + 'static;
    methods: HashMap<String, Box<Fn(&JsonRpcRequest) -> Result<Json, ErrorCode> + 'static>>
}

impl JsonRpcServer {
    pub fn new() -> JsonRpcServer {
        JsonRpcServer {
            methods: HashMap::new()
        }
    }
    
    pub fn register_str<F>(&mut self, name: &str, f: F)
        where F: Fn(&JsonRpcRequest) -> Result<Json, ErrorCode> + 'static {
        self.methods.insert(name.to_string(), Box::new(f));
    }

    fn _handle_single(&self, req: &rustc_serialize::json::Object) 
        -> Result<Option<Json>, ErrorCode> {
        //Exclude version check before parse
        match req.get("jsonrpc") {
            Some(o) => match o.as_string() {
                Some(s) => if s != "2.0" {
                    return Err(ErrorCode::InvalidRequest)
                } else {},
                _ => return Err(ErrorCode::InvalidRequest)
            },
            _ => return Err(ErrorCode::InvalidRequest)
        };
        let request = JsonRpcRequest {
            //Required
            method: match req.get("method") {
                Some(json) => match json.as_string() {
                    Some(s) => s.to_string(),
                    _ => return Err(ErrorCode::InvalidRequest)
                },
                _ => return Err(ErrorCode::InvalidRequest)
            },
            //Optional
            params: match req.get("params") {
                Some(json) => match json {
                    &Json::Array(_) => Some(json.clone()),
                    &Json::Object(_) => Some(json.clone()),
                    _ => return Err(ErrorCode::InvalidRequest),
                },
                None => None
            },
            //Optional
            id: match req.get("id") {
                Some(json) => match *json {
                    //Allow only primitives
                    Json::String(_) | Json::U64(_) 
                        | Json::I64(_) | Json::Null => Some(json.clone()),
                    _ => return Err(ErrorCode::InvalidRequest)
                },
                None => None
            }
        };
        //id == None -> Notification (method can return only NULL)
        let response = match self.finalize_request(&request) {
            Ok(ref s) 
                if request.id == None && *s == Json::Null => None,
            //No request id, but method returned some data...
            Ok(_) 
                if request.id == None => Some(JsonRpcResponse::new_error(ErrorCode::InternalError, None, request.id)),
            Ok(s) => Some(JsonRpcResponse::new_result(&request, s)),
            Err(e) => Some(JsonRpcResponse::new_error(e, None, request.id))
        };
        match response {
            Some(s) => Ok(Some(s.to_json())),
            None => Ok(None)
        }
        // Ok(Some(response.to_json()))
    }

    fn finalize_request(&self, request: &JsonRpcRequest) 
        -> Result<Json, ErrorCode> {
        //tutaj juz mozna zwrocic informacje z kodem bo znamy request
        let method_invoke = match self.methods.get(&request.method) {
            Some(s) => s,
            _ => { 
                println!("Requested method '{}' not found!", request.method);
                return Err(ErrorCode::MethodNotFound)
            }
        };
        method_invoke(&request)
    }

    fn _handle_multiple(&self, array: &rustc_serialize::json::Array) -> Result<Option<Json>, ErrorCode> {
        let mut response_vector = Vec::new();
        if array.len() == 0 {
            return Err(ErrorCode::InvalidRequest);
        }
        for request in array {
            println!("Processing {}", request);
            let response = if request.is_object() {
                match self._handle_single(request.as_object().unwrap()) {
                    Ok(ok) => ok,
                    //Skoro jest err, to id jest nieznane
                    Err(err) => Some(JsonRpcResponse::new_error(err, None, None).to_json())
                }
            } else {
                Some(JsonRpcResponse::new_error(ErrorCode::InvalidRequest, None, None).to_json())
            };
            //Skip notifications in response
            if response != None {
                response_vector.push(response);
            }
        }

        //All notifications nothing to respond
        if response_vector.len() == 0 {
            Ok(None)
        } else {
            Ok(Some(response_vector.to_json()))
        }
    }

    fn _handle_request(&self, request: &str)
        -> Result<Option<Json>, ErrorCode> {
        let request_json = match Json::from_str(&request) {
            Ok(o) => o,
            Err(_) => return Err(ErrorCode::ParseError)
        };

        //for now only plain object support
        match request_json {
            Json::Object(ref s) => self._handle_single(s),
            Json::Array(ref a) => self._handle_multiple(a), 
            _ => return Err(ErrorCode::InvalidRequest)
        }
    }
    //request: Raw json
    //return: Raw json
    pub fn handle_request(&self, request: String) -> String {
        let result = self._handle_request(&request);
        match result {
            Ok(Some(resp)) => resp.to_json().to_string(),
            //Notification, no response
            Ok(None) => "".to_string(),
            Err(err) => JsonRpcResponse::new_error(err, None, None)
                .to_json().to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustc_serialize::json::{self, Json, ToJson};

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

        assert!("".to_string() == server.handle_request("{\"jsonrpc\": \"2.0\",                                                         \"method\": \"update\",
                                        \"params\": [1,2,3,4,5]}".to_string()));
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
        let mut server = JsonRpcServer::new();
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

