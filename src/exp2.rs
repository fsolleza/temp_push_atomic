use rand::Rng;
use rtrb::*;
use std::{env, mem};

use std::sync::{
    atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering::SeqCst},
    Arc, RwLock,
};
use std::thread::{self};
use std::time;

pub struct Base {
    data: [u64; 256],
    len: usize,
    alen: AtomicUsize,
    aver: AtomicUsize,
    ver: usize,
}

pub struct Read {
    data: [u64; 256],
    len: usize,
    version: usize,
}

impl Base {
    fn new() -> Self {
        Base {
            data: [0u64; 256],
            len: 0,
            alen: AtomicUsize::new(0),
            aver: AtomicUsize::new(0),
            ver: 0,
        }
    }

    fn atomic_push(&mut self, item: u64) {
        if self.len == 256 {
            self.aver.fetch_add(1, SeqCst);
            self.alen.store(0, SeqCst);
            self.len = 0;
        }
        self.data[self.len] = item;
        self.len += 1;
        self.alen.store(self.len, SeqCst);
    }

    fn push(&mut self, item: u64) {
        if self.len == 256 {
            self.ver += 1;
            self.len = 0;
        }
        self.data[self.len] = item;
        self.len += 1;
    }

    fn read_server(&self) -> Result<Read, &str> {
        for i in 0..3 {
            let v = self.aver.load(SeqCst);
            let mut data = [0u64; 256];
            let len = self.alen.load(SeqCst);
            data[..len].copy_from_slice(&self.data[..len]);
            if v == self.aver.load(SeqCst) {
                let r = Read {
                    data,
                    len,
                    version: v,
                };
                return Ok(r);
            }
        }
        Err("can't read")
    }

    fn read(&self) -> Result<Read, &str> {
        for i in 0..3 {
            let v = self.ver;
            let mut data = [0u64; 256];
            let len = self.len;
            data[..len].copy_from_slice(&self.data[..len]);
            if v == self.ver {
                let r = Read {
                    data,
                    len,
                    version: v,
                };
                return Ok(r);
            }
        }
        Err("can't read")
    }
}

/// Sets up a read server that updates a pointer each period.
fn read_server(
    active: Arc<AtomicBool>,
    ptr: Arc<AtomicPtr<Read>>,
    data: &Base,
    sleep_dur_micros: u64,
) {
    let mut update_cnt = 0;
    let mut v = 0;
    let mut sum = 0;
    while active.load(SeqCst) {
        match data.read_server() {
            Ok(x) => {
                // Take a write lock and update the new pointer with most recent read data
                let r = Box::into_raw(Box::new(x));
                ptr.store(r, SeqCst);
                let r = unsafe { r.as_ref().unwrap() };
                update_cnt += 1;
                v = r.version;
                sum += r.data[..r.len].iter().sum::<u64>() % 1024;
            }
            Err(x) => {
                println!("{}", x);
            }
        }
        thread::sleep(time::Duration::from_micros(sleep_dur_micros));
    }
    //println!("update_cnt {} ", update_cnt);
    //println!("update_cnt {} last version {} sum {}", update_cnt, v, sum);
}

/// Run a reader that repeatedly reads the data written by the read_server. Takes a readlock and
/// clones that data
fn read_server_loop(
    active: Arc<AtomicBool>,
    ptr: Arc<AtomicPtr<Read>>,
    read_count: Arc<AtomicU64>,
    sink: Arc<AtomicU64>,
    data: &Base,
) {
    let mut sum = 0;
    let mut ctr = 0;
    let mut v = 0;
    while active.load(SeqCst) {
        let read = unsafe { ptr.load(SeqCst).as_ref().unwrap() };
        sum += read.data[..read.len].iter().sum::<u64>();
        v = read.version;
        ctr += 1;
    }
    read_count.fetch_add(ctr, SeqCst);
    //println!("{:?} {:?} {:?}", sum, ctr, v);
    sink.fetch_add(sum % 1024, SeqCst);
}

/// Run a reader that repeatedly calls the read() function which uses no syncrhonization except for
/// the version number (can't be avoided)
fn read_loop(
    active: Arc<AtomicBool>,
    read_count: Arc<AtomicU64>,
    sink: Arc<AtomicU64>,
    data: &Base,
) {
    let mut sum = 0;
    let mut ctr = 0;
    while active.load(SeqCst) {
        match data.read() {
            Ok(read) => {
                sum += read.data[..read.len].iter().sum::<u64>();
                ctr += 1;
            }
            Err(_) => {}
        }
    }
    read_count.fetch_add(ctr, SeqCst);
    sink.fetch_add(sum % 1024, SeqCst);
}

/// A write loop that forces an atomic boolean every time a push happens
fn atomic_write_loop(
    active: Arc<AtomicBool>,
    data: &mut Base,
    to_write: &[u64],
    push_sync_freq: usize,
) -> time::Duration {
    let mut counter = 0;
    let l = to_write.len();
    let now = time::Instant::now();
    for i in 0..NPUSHES {
        // This function increments an atomic counter
        if i % push_sync_freq == 0 {
            data.atomic_push(to_write[i % l]);
        } else {
            data.push(to_write[i % l]);
        }
    }
    let elapsed = now.elapsed();
    active.store(false, SeqCst);
    elapsed
}

/// A write loop that uses no syncrhonization
fn write_loop(active: Arc<AtomicBool>, data: &mut Base, to_write: &[u64]) -> time::Duration {
    let mut counter = 0;
    let l = to_write.len();
    let now = time::Instant::now();
    for i in 0..NPUSHES {
        // This function increments a non-atomic counter
        data.push(to_write[i % l]);
    }
    let elapsed = now.elapsed();
    active.store(false, SeqCst);
    elapsed
}

/// Runs the benchmark without atomics
fn runner(readers: usize, read_count: Arc<AtomicU64>, sink: Arc<AtomicU64>) -> time::Duration {
    let mut to_write = [0u64; 1234];
    rand::thread_rng().fill(&mut to_write[..]);
    let data = Arc::new(Base::new());
    let active = Arc::new(AtomicBool::new(true));

    let mut handles = Vec::new();
    for _ in 0..readers {
        let dc = data.clone();
        let ac = active.clone();
        let sc = sink.clone();
        let rc = read_count.clone();
        handles.push(thread::spawn(move || {
            let dc = dc;
            // uses unsynched reader (except for version number)
            read_loop(ac, rc, sc, &dc);
        }));
    }

    let dc = data.clone();
    let ac = active.clone();
    let data: &mut Base = unsafe { (Arc::as_ptr(&dc) as *mut Base).as_mut().unwrap() };
    // uses unsynched writer (except for version number)
    let dur = write_loop(ac, data, &to_write);
    for h in handles {
        h.join();
    }
    dur
}

fn runner_server(
    readers: usize,
    read_count: Arc<AtomicU64>,
    sink: Arc<AtomicU64>,
    sleep_dur_micros: u64,
    push_sync_freq: usize,
) -> time::Duration {
    let mut to_write = [0u64; 1234];
    rand::thread_rng().fill(&mut to_write[..]);
    let data = Arc::new(Base::new());
    let active = Arc::new(AtomicBool::new(true));
    let read_ptr = Arc::new(AtomicPtr::new(Box::into_raw(Box::new(
        data.read().unwrap(),
    ))));
    let mut handles = Vec::new();

    // Run server
    let dc = data.clone();
    let ac = active.clone();
    let pc = read_ptr.clone();
    handles.push(thread::spawn(move || {
        read_server(ac, pc, &dc, sleep_dur_micros);
    }));

    // Run each reader
    for _ in 0..readers {
        let dc = data.clone();
        let ac = active.clone();
        let pc = read_ptr.clone();
        let rc = read_count.clone();
        let sc = sink.clone();
        handles.push(thread::spawn(move || {
            let dc = dc;
            // run the readers that pull from the read server
            read_server_loop(ac, pc, rc, sc, &dc);
        }));
    }

    // Run the writer
    let dc = data.clone();
    let ac = active.clone();
    let data: &mut Base = unsafe { (Arc::as_ptr(&dc) as *mut Base).as_mut().unwrap() };
    // run the atomic writer
    let dur = atomic_write_loop(ac, data, &to_write, push_sync_freq);
    for h in handles {
        h.join();
    }
    dur
}

fn mean(list: &[f64]) -> f64 {
    let sum: f64 = Iterator::sum(list.iter());
    sum / list.len() as f64
}

fn median(v: &[f64]) -> f64 {
    let mut list = Vec::new();
    list.extend_from_slice(v);
    list.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let len = list.len();
    let mid = len / 2;
    if len % 2 == 0 {
        mean(&list[(mid - 1)..(mid + 1)])
    } else {
        list[mid]
    }
}

const ITERS: usize = 30; // number of iterations of the experiment to run
const NTHREADS: usize = 36; // number of concurrent reader threads
const NPUSHES: usize = 100_000_000; // how many pushes each experiment should run for

fn bench_no_sync() {
    let sink = Arc::new(AtomicU64::new(0));
    let read_count = Arc::new(AtomicU64::new(0));
    let mut v = Vec::new();
    for i in 0..ITERS {
        let d = runner(NTHREADS, read_count.clone(), sink.clone());
        v.push(d.as_secs_f64());
    }
    println!("average: {}", mean(v.as_slice()));
    println!("median: {}", median(v.as_slice()));
    println!("read_count: {:?}", read_count);
    println!("sink: {:?}", sink);
}

fn bench_read_server(sleep_dur_micros: u64, push_sync_freq: usize) {
    let sink = Arc::new(AtomicU64::new(0));
    let read_count = Arc::new(AtomicU64::new(0));
    let mut v = Vec::new();
    for i in 0..ITERS {
        let d = runner_server(
            NTHREADS,
            read_count.clone(),
            sink.clone(),
            sleep_dur_micros,
            push_sync_freq,
        );
        v.push(d.as_secs_f64());
    }
    println!("average: {}", mean(v.as_slice()));
    println!("median: {}", median(v.as_slice()));
    println!("read_count: {:?}", read_count);
    println!("sink: {:?}", sink);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let dur_micros = &args[1].parse::<u64>().unwrap();
    let push_sync_freq = &args[2].parse::<usize>().unwrap();

    // bench_no_sync();
    bench_read_server(*dur_micros, *push_sync_freq);
}
