
use std::time::{ SystemTime, UNIX_EPOCH };
use std::sync::mpsc;
use std::thread;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;
use std::fmt;
use std::error::Error;



#[derive(Debug)]
enum State {
    Open,
    Closed,
    HalfOpen
}

#[derive(Debug)]
enum MyError<E> {
    FunctionError(E),
    TimeoutError,
}

impl<E: fmt::Debug> fmt::Display for MyError<E> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MyError::FunctionError(e) => write!(f, "FunctionError: {:?}", e),
            MyError::TimeoutError => write!(f, "TimeoutError"),
        }
    }
}

#[derive(Debug)]
struct CircuitBreaker {
    state: State,
    failure_threshold: u32,
    failure_count: u32,
    last_failure_time: u64,
    timeout: u64,
    recovery_time: u64,
    open_success_count: u64,
    open_threshold_count: u64,
}

impl CircuitBreaker {
    fn new(failure_threshold: u32, timeout: u64, recovery_time: u64, open_threshold_count: u64) -> Self {
        CircuitBreaker {
            state: State::Closed,
            failure_threshold,
            failure_count: 0,
            last_failure_time: 0,
            recovery_time,
            timeout,
            open_success_count: 0,
            open_threshold_count,
        }
    }

  

    fn call<F, R, E>(&mut self, func: F) -> Result<Option<R>, MyError<E>> 
    where
        F: FnOnce() -> Result<R, E> + Send + 'static,
        R: Send + 'static,
        E: Send + 'static,
    {
        match self.state {
            State::Open => {
                self.handle_open_state()?;
                Ok(None)
            }
            State::Closed => {
                let res = self.handle_closed_state(func)?;
                Ok(Some(res))
            }
            State::HalfOpen => {
                let res = self.handle_half_open_state(func)?;
                Ok(Some(res))
            }
        }
    }

    fn handle_open_state<E>(&mut self) -> Result<(), MyError<E>> {
        if self.last_failure_time >= self.recovery_time {
            self.state = State::HalfOpen;
            self.open_success_count = 0;
            self.failure_count = 0;
            
            Ok(())
        } else {
           Err(MyError::TimeoutError)
        }

    }

    fn handle_half_open_state<F, R, E>(&mut self, func: F) -> Result<R, MyError<E>> 
    where
       F: FnOnce() -> Result<R, E> + Send + 'static,
       R: Send + 'static,
       E: Send + 'static,
       {

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            // Simulate some work.
            let res = func();
            tx.send(res).unwrap();
        });

        match rx.recv_timeout(Duration::from_millis(self.timeout)) {
            Ok(res) => {
                match res {
                   Ok(data) => {
                    self.open_success_count += 1;
                    if self.open_success_count >= self.open_threshold_count {
                        self.state = State::Closed;
                        self.open_success_count = 0;
                        self.failure_count = 0;
                    }
                    
                    Ok(data)
                   },
                   Err(e) => {
                    self.last_failure_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u64;
                    self.failure_count = 1;
                    self.state = State::Open;
                    
                    Err(MyError::FunctionError(e))
                   },
                }
            }
            _ => { 
                self.state = State::Open;
                self.last_failure_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u64;
                self.failure_count = 1;
                Err(MyError::TimeoutError)
            },
        }

       }

    fn handle_closed_state<F,R,E>(&mut self, func: F) -> Result<R, MyError<E>> 
    where
        F: FnOnce() -> Result<R, E> + Send + 'static,
        R: Send + 'static,
        E: Send + 'static,
       
        {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            // Simulate some work.
            let res = func();
            tx.send(res).unwrap();
        });

        match rx.recv_timeout(Duration::from_millis(self.timeout)) {
            Ok(res) => {
                match res { 
                   Ok(data) => {
                    self.failure_count = 0;
                    self.state = State::Closed;
                    Ok(data)
                   },
                   Err(e) => {
                    self.last_failure_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u64;
                    self.failure_count += 1;
                    if self.failure_count > self.failure_threshold {
                        self.state = State::Open;
                    }
                    Err(MyError::FunctionError(e))
                   },
                }
            }
            _ => { 
                self.state = State::Open;
                self.last_failure_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u64;
                self.failure_count = 1;
                Err(MyError::TimeoutError)
            },
        }
    }

}    

fn unreliable_service() -> Result<String, Box<dyn Error + Send>> {
    let start = SystemTime::now();
    let since_the_epoch = start.duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let in_sec = since_the_epoch.as_secs();

    if in_sec % 2 == 0 {
        Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "service failed")))
    } else {
        Ok("Success!".to_string())
    }
}

fn main() {
    println!("Hello, world!");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unreliable_service() {
        let mut cb = CircuitBreaker::new(3, 1000, 5000, 2);

        for _ in 0..10 {
            match cb.call(|| unreliable_service()) {
                Ok(Some(res)) => println!("Service returned: {}", res),
                Ok(None) => println!("Service is in open state"),
                Err(MyError::FunctionError(e)) => println!("Service failed with error: {:?}", e),
                Err(MyError::TimeoutError) => println!("Service timed out"),
            }
            thread::sleep(Duration::from_secs(1));
        }
    }

}