use hyper::{Client, Request, Body, Version};
use hyper::Uri;
use hyper::http::uri::{Scheme, Authority, PathAndQuery};
use hyper::client::connect::HttpConnector;
use std::time;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::io;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    // We made a vector to write down the information about, how fast our requests get responce
    let messages_time = Arc::new(Mutex::new(Vec::new()));

    // Here we take user input and convert it to i32. After that we make sure, input is between 1
    // and 100. If not - we end program.
    let mut user_input = String::new();
    println!("How many requests should we send to a server (pay attention, it should be 1 to 100:");
    io::stdin().read_line(&mut user_input).expect("Something gone wrong");
    let requests_number: i32 = user_input.trim().parse().expect("thats not a number");
    if requests_number > 100 || requests_number < 1 {
        emergency_shutdown(Arc::clone(&messages_time), SystemTime::now());
    }

    // NOw we set up our HttpConnector. It will have connect timeout of 2 secs
    let second = time::Duration::from_secs(2);
    let mut http = HttpConnector::new();
    http.set_connect_timeout(Some(second));

    // Thats our client for requests
    let client = Client::builder().http2_only(true).build::<>(http);

    // We get our start time
    let start = SystemTime::now();

    // I desided to make vector of tasks. So, we send all requests almost at the same time
    let mut tasks = Vec::new();

    // we make n requests and push them into vec.
    for _ in 0..requests_number {
        let messages_time = Arc::clone(&messages_time);
        let client = client.clone();
        let task = tokio::spawn ( async move {
            request_builder(client, messages_time, start).await.expect("some problem here");
        });
        tasks.push(task);
    }

    // Now we make requests and w8 for answer.
    for task in tasks {
            task.await?;
    }

    // If our program done well - we write all the results
    let message = "Программа завершилась успешно".to_string();
    statistic_write(message, Arc::clone(&messages_time), start);

    // Huuray
    Ok(())
}

// That is a function for request building
async fn request_builder (client: hyper::client::Client<HttpConnector>,
                          messages_time: Arc<Mutex<Vec<time::Duration>>>,
                        start_working_time: time::SystemTime)
    -> Result<(), Box<dyn std::error::Error + Send + Sync>>{

    // We get Uri
    let uri = Uri::builder()
            .scheme(Scheme::HTTP)
            .authority(Authority::from_static("127.0.0.1:3000"))
            .path_and_query(PathAndQuery::from_static("/"))
            .build()?;
    // and request
        let mut req = Request::get(uri)
            .body(Body::empty())?;

    // We set HTTP 2 version manually
        *req.version_mut() = Version::HTTP_2;


    // Thats when we start our request
        let start = SystemTime::now();

    // We also check out, if we w8 not to long for an answer. I desided to set 4 sec.
        let request = tokio::time::timeout(tokio::time::Duration::from_secs(5),
                                           client.request(req)).await;

    // Any error - we shut down
        match request {
        Ok(t) => match t {
            Ok(_) => (),
            Err(_) => emergency_shutdown(Arc::clone(&messages_time), start_working_time),
        }
        Err(_) =>  emergency_shutdown(Arc::clone(&messages_time), start_working_time),

    }

    // We got our answer and check time, when it happens
    let stop = SystemTime::now();

    // stop - start is elapced time
    let difference = stop.duration_since(start).expect("for some reason,\
    we could not count how much time request work");

    // Now we need to add our difference to vec
    let mut data1 = messages_time.lock().expect("unable to get data from mutex");
             data1.push(difference);

    Ok(())


}

// This function is made so, we can save all the information in case of error
fn emergency_shutdown (messages_time: Arc<Mutex<Vec<time::Duration>>>, start_working_time:time::SystemTime) {
    let message = "При работе программы произошла ошибка".to_string();
    statistic_write(message, Arc::clone(&messages_time), start_working_time);
    std::process::exit(1);
}

// This is function, that write all information into file
fn statistic_write (message: String, messages_time: Arc<Mutex<Vec<time::Duration>>>,
                    server_start_time: time::SystemTime) {
    // If we got there - we need to stop time, cause we won't talk to server anymore
    let stop = SystemTime::now();
    let difference = stop.duration_since(server_start_time).expect("for some reason,\
    we could not count how much time request work in statistic_write function");

    // We make a file
    let srcdir = PathBuf::from("file.txt");
    let mut file = File::create(srcdir).expect("unable to create file");

    // First of all - if we have empty vec - that means, we didnt finished a single request.
    let messages_time1 = messages_time.lock().expect("Unable to open mutex");
    if messages_time1.len() == 0 {
        writeln!(file, "При работе программы произошла ошибка. Ни одного запроса не было обработано")
            .expect("Проблемы с записью в файл 1");
    } else {

        // Most likely, our max w8 time will be last in our vec. And min w8 time - first
        let mut total_requests_time = time::Duration::from_secs(0);
        let mut max_waiting_time = messages_time1[messages_time1.len()-1];
        let mut min_waiting_time = messages_time1[0];

        // We get summ of working times and to be sure - check out, if we have right max and
        // min waiting time
        for i in 0..messages_time1.len() {
            total_requests_time += messages_time1[i];
            if messages_time1[i]>max_waiting_time {
                max_waiting_time = messages_time1[i];
            }else if messages_time1[i]<min_waiting_time{
                min_waiting_time = messages_time1[i];
            }
        }

        let average_request_time = total_requests_time/messages_time1.len() as u32;

        // We just need to write all the information.
        writeln!(file, "{}", message).expect("Проблемы с записью в файл 2");
        writeln!(file, "Общее количество запросов, на которые получены ответы: {:?}", messages_time1.len())
            .expect("Проблемы с записью в файл 3");
        writeln!(file, "Общее время запроса данных у сервера: {:?}", difference)
            .expect("Проблемы с записью в файл 4");
        writeln!(file, "Минимальное время запроса данных у сервера: {:?}", min_waiting_time)
            .expect("Проблемы с записью в файл 5");
        writeln!(file, "Среднее время запроса данных у сервера: {:?}", average_request_time)
            .expect("Проблемы с записью в файл 6");
        writeln!(file, "Максимальное время запроса данных у сервера: {:?}", max_waiting_time)
            .expect("Проблемы с записью в файл 7");
    }
}