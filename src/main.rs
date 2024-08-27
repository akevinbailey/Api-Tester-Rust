use std::env;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use reqwest::{Client, ClientBuilder, header};
use tokio;
use futures::future::join_all;

fn print_help() {
    println!("Usage:");
    println!("  api-tester [URL] [arguments]");
    println!("Required arguments:");
    println!("  [URL]                   - Server URL.");
    println!("Optional Arguments:");
    println!("  -totalCalls [value]     - Total number of calls across all threads. Default is 10000.");
    println!("  -numThreads [value]     - Number of threads. Default is 12.");
    println!("  -sleepTime [value]      - Sleep time in milliseconds between calls within a thread. Default is 0.");
    println!("  -requestTimeOut [value] - HTTP request timeout in milliseconds. Default is 10000.");
    println!("  -connectTimeOut [value] - HTTP request timeout in milliseconds. Default is 20000.");
    println!("  -reuseConnects          - Add the request 'Connection: keep-alive' header.");
    println!("  -keepConnectsOpen       - Force a new connection with every request (not advised).");
    println!("Help:");
    println!("  -? or --help - Display this help message.");
}

async fn fetch_data(client: Client, response_times: Arc<Mutex<Vec<f64>>>, url: String, sleep_time: Duration, keep_connects_open: bool,
                    thread_id: usize, num_calls: usize) {
    for i in 0..num_calls {
        let start_time = Instant::now();
        let response = client.get(&url).send().await;
        let end_time = Instant::now();

        let response_time = (end_time - start_time).as_secs_f64() * 1000.0;

        let status:String;
        match response {
            Ok(resp) => {
                status = resp.status().to_string();
                if !keep_connects_open {
                    let _ = resp.bytes().await;
                }
                println!("Thread {:2}.{:<6} - Success: {} - Response time: {:.2} ms", thread_id, i, status, response_time);
            }
            Err(err) => {
                println!("Thread {:2}.{:<6} - Request failed: {} - Response time: {:.2} ms", thread_id, i, err, response_time);
            }
        }

        response_times.lock().unwrap().push(response_time);

        tokio::time::sleep(sleep_time).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Error: No command line argument provided.");
        print_help();
        return Ok(());
    }

    if args.contains(&"-?".to_string()) || args.contains(&"--help".to_string()) {
        print_help();
        return Ok(());
    }

    let url = &args[1];
    if !url.to_lowercase().starts_with("http") {
        println!("Error: \"{}\" is not a valid URL", url);
        print_help();
        return Ok(());
    }

    let mut total_calls = 10000;
    let mut num_threads = 12;
    let mut sleep_time = Duration::from_millis(0);
    let mut request_timeout = Duration::from_millis(10000);
    let mut connect_timeout = Duration::from_millis(30000);
    let mut reuse_connects = false;
    let mut keep_connects_open = false;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "-totalCalls" => {
                total_calls = args[i + 1].parse().expect("Invalid integer for totalCalls");
                i += 2;
            }
            "-numThreads" => {
                num_threads = args[i + 1].parse().expect("Invalid integer for numThreads");
                i += 2;
            }
            "-sleepTime" => {
                sleep_time = Duration::from_millis(args[i + 1].parse().expect("Invalid integer for sleepTime"));
                i += 2;
            }
            "-requestTimeOut" => {
                request_timeout = Duration::from_millis(args[i + 1].parse().expect("Invalid integer for requestTimeOut"));
                i += 2;
            }
            "-connectTimeOut" => {
                connect_timeout = Duration::from_millis(args[i + 1].parse().expect("Invalid integer for connectTimeOut"));
                i += 2;
            }
            "-reuseConnects" => {
                reuse_connects = true;
                i += 1;
            }
            "-keepConnectsOpen" => {
                keep_connects_open = true;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    let mut client_builder = ClientBuilder::new()
        .timeout(request_timeout)
        .connect_timeout(connect_timeout)
        .pool_max_idle_per_host(num_threads * 10);

    if url.to_lowercase().starts_with("https") {
        client_builder = client_builder.danger_accept_invalid_certs(true);
    }

    let mut header = header::HeaderMap::new();
    if reuse_connects {
        header.insert("Connection", header::HeaderValue::from_static("keep-alive"));
        client_builder = client_builder.default_headers(header);
    } else {
        header.insert("Connection", header::HeaderValue::from_static("close"));
        client_builder = client_builder.default_headers(header);
        client_builder = client_builder.pool_max_idle_per_host(0);
    }

    let client = client_builder.build()?;

    let response_times = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    let start_time = Instant::now();

    for i in 0..num_threads {
        let num_calls = total_calls / num_threads + if i < total_calls % num_threads { 1 } else { 0 };
        let client = client.clone();
        let response_times = Arc::clone(&response_times);
        let url = url.to_string();

        handles.push(tokio::spawn(async move {
            fetch_data(client, response_times, url, sleep_time, keep_connects_open, i, num_calls).await;
        }));
    }

    join_all(handles).await;

    let end_time = Instant::now();
    let total_time = (end_time - start_time).as_secs_f64();

    let requests_per_second = total_calls as f64 / total_time;

    let response_times = response_times.lock().unwrap();
    let average_response_time = response_times.iter().sum::<f64>() / response_times.len() as f64;

    println!("Total test time: {:.2} s", total_time);
    println!("Average response time: {:.2} ms", average_response_time);
    println!("Average requests per second: {:.2}", requests_per_second);

    println!("All threads have finished.");

    Ok(())
}