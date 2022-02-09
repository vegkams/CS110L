mod request;
mod response;

use clap::Parser;
use rand::{Rng, SeedableRng};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use std::sync::Arc;
use std::io::{Error, ErrorKind};

/// Contains information parsed from the command-line invocation of balancebeam. The Clap macros
/// provide a fancy way to automatically construct a command-line argument parser.
#[derive(Parser, Debug)]
#[clap(about = "Fun with load balancing")]
struct CmdOptions {
    #[clap(
        short,
        long,
        help = "IP/port to bind to",
        default_value = "0.0.0.0:1100"
    )]
    bind: String,
    #[clap(short, long, help = "Upstream host to forward requests to")]
    upstream: Vec<String>,
    #[clap(
        long,
        help = "Perform active health checks on this interval (in seconds)",
        default_value = "10"
    )]
    active_health_check_interval: usize,
    #[clap(
        long,
        help = "Path to send request to for active health checks",
        default_value = "/"
    )]
    active_health_check_path: String,
    #[clap(
        long,
        help = "Maximum number of requests to accept per IP per minute (0 = unlimited)",
        default_value = "0"
    )]
    max_requests_per_minute: usize,
}

/// Contains information about the state of balancebeam (e.g. what servers we are currently proxying
/// to, what servers have failed, rate limiting counts, etc.)
///
/// You should add fields to this struct in later milestones.
struct ProxyState {
    /// How frequently we check whether upstream servers are alive (Milestone 4)
    #[allow(dead_code)]
    active_health_check_interval: usize,
    /// Where we should send requests when doing active health checks (Milestone 4)
    #[allow(dead_code)]
    active_health_check_path: String,

    upstreams_state: RwLock<UpstreamsState>,
    /// Maximum number of requests an individual IP can make in a minute (Milestone 5)
    #[allow(dead_code)]
    max_requests_per_minute: usize,
    /// Addresses of servers that we are proxying to
    upstream_addresses: RwLock<Vec<String>>,
}

struct UpstreamsState {
    num_upstreams: usize,
    status: Vec<bool>,
}

impl UpstreamsState {
    fn new(num_upstreams: usize) -> UpstreamsState {
        UpstreamsState {
            num_upstreams: num_upstreams,
            status: vec![true; num_upstreams], 
        }
    }

    fn is_alive(&self, idx: usize) -> bool {
        self.status[idx]
    }

    fn all_dead(&self) -> bool {
        self.num_upstreams == 0
    }

    fn set_dead(&mut self, idx: usize) {
        if self.is_alive(idx) {
            self.status[idx] = false;
            self.num_upstreams -= 1;
        }
    }

    fn set_alive(&mut self, idx: usize) {
        if !self.is_alive(idx) {
            self.status[idx] = true;
            self.num_upstreams += 1;
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize the logging library. You can print log messages using the `log` macros:
    // https://docs.rs/log/0.4.8/log/ You are welcome to continue using print! statements; this
    // just looks a little prettier.
    if let Err(_) = std::env::var("RUST_LOG") {
        std::env::set_var("RUST_LOG", "debug");
    }
    pretty_env_logger::init();

    // Parse the command line arguments passed to this program
    let options = CmdOptions::parse();
    if options.upstream.len() < 1 {
        log::error!("At least one upstream server must be specified using the --upstream option.");
        std::process::exit(1);
    }

    // Start listening for connections
    let mut listener = match TcpListener::bind(&options.bind).await {
        Ok(listener) => listener,
        Err(err) => {
            log::error!("Could not bind to {}: {}", options.bind, err);
            std::process::exit(1);
        }
    };
    log::info!("Listening for requests on {}", options.bind);

    let num_upstreams = options.upstream.len();
    // Handle incoming connections
    let state = ProxyState {
        upstream_addresses: RwLock::new(options.upstream),
        active_health_check_interval: options.active_health_check_interval,
        active_health_check_path: options.active_health_check_path,
        upstreams_state: RwLock::new(UpstreamsState::new(num_upstreams)),
        max_requests_per_minute: options.max_requests_per_minute,
    };

    let shared_state = Arc::new(state);

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let shared_state_ref = shared_state.clone();
                // Handle the connection!
                tokio::spawn(async move {
                    handle_connection(stream, shared_state_ref).await
                });
            }
            Err(_) => { break; }
        }
    }
}


async fn active_health_check(state: Arc<ProxyState>) {

}

async fn check_server_status(state: Arc<ProxyState, idx: usize, path: &String>) -> Option<bool> {
    
}


async fn connect_to_upstream(state: Arc<ProxyState>) -> Result<TcpStream, std::io::Error> {
    loop {
        let mut rng = rand::rngs::StdRng::from_entropy();
        let upstream_addresses_r = state.upstream_addresses.read().await;
        let upstream_idx = rng.gen_range(0, upstream_addresses_r.len());
        let upstream_ip = &upstream_addresses_r[upstream_idx];

        match TcpStream::connect(upstream_ip).await {
            Err(err) => { log::warn!("Failed to connect to upstream: {:?}", err);
                          // Free the read lock
                          drop(upstream_addresses_r);
                          // Aquire a write lock
                          let mut upstream_addresses_w = state.upstream_addresses.write().await;
                          upstream_addresses_w.remove(upstream_idx);
                          if upstream_addresses_w.is_empty() {
                              return Err(std::io::Error::new(ErrorKind::Other, "All the upstream servers are down!"))
                          }
                        },
            Ok(s) => return Ok(s),
        }
    }
    // TODO: implement failover (milestone 3)
}

async fn send_response(client_conn: &mut TcpStream, response: &http::Response<Vec<u8>>) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!("{} <- {}", client_ip, response::format_response_line(&response));
    if let Err(error) = response::write_to_stream(&response, client_conn).await {
        log::warn!("Failed to send response to client: {}", error);
        return;
    }
}

async fn handle_connection(mut client_conn: TcpStream, state: Arc<ProxyState>) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!("Connection received from {}", client_ip);

    // Open a connection to a random destination server
    let mut upstream_conn = match connect_to_upstream(state).await {
        Ok(stream) => stream,
        Err(_error) => {
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
    };
    let upstream_ip = client_conn.peer_addr().unwrap().ip().to_string();

    // The client may now send us one or more requests. Keep trying to read requests until the
    // client hangs up or we get an error.
    loop {
        // Read a request from the client
        let mut request = match request::read_from_stream(&mut client_conn).await {
            Ok(request) => request,
            // Handle case where client closed connection and is no longer sending requests
            Err(request::Error::IncompleteRequest(0)) => {
                log::debug!("Client finished sending requests. Shutting down connection");
                return;
            }
            // Handle I/O error in reading from the client
            Err(request::Error::ConnectionError(io_err)) => {
                log::info!("Error reading request from client stream: {}", io_err);
                return;
            }
            Err(error) => {
                log::debug!("Error parsing request: {:?}", error);
                let response = response::make_http_error(match error {
                    request::Error::IncompleteRequest(_)
                    | request::Error::MalformedRequest(_)
                    | request::Error::InvalidContentLength
                    | request::Error::ContentLengthMismatch => http::StatusCode::BAD_REQUEST,
                    request::Error::RequestBodyTooLarge => http::StatusCode::PAYLOAD_TOO_LARGE,
                    request::Error::ConnectionError(_) => http::StatusCode::SERVICE_UNAVAILABLE,
                });
                send_response(&mut client_conn, &response).await;
                continue;
            }
        };
        log::info!(
            "{} -> {}: {}",
            client_ip,
            upstream_ip,
            request::format_request_line(&request)
        );

        // Add X-Forwarded-For header so that the upstream server knows the client's IP address.
        // (We're the ones connecting directly to the upstream server, so without this header, the
        // upstream server will only know our IP, not the client's.)
        request::extend_header_value(&mut request, "x-forwarded-for", &client_ip);

        // Forward the request to the server
        if let Err(error) = request::write_to_stream(&request, &mut upstream_conn).await {
            log::error!("Failed to send request to upstream {}: {}", upstream_ip, error);
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
        log::debug!("Forwarded request to server");

        // Read the server's response
        let response = match response::read_from_stream(&mut upstream_conn, request.method()).await {
            Ok(response) => response,
            Err(error) => {
                log::error!("Error reading response from server: {:?}", error);
                let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
                send_response(&mut client_conn, &response).await;
                return;
            }
        };
        // Forward the response to the client
        send_response(&mut client_conn, &response).await;
        log::debug!("Forwarded response to client");
    }
}