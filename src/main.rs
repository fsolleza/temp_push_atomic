use rand::thread_rng;
use rand::Rng;
use std::panic;
use std::sync::{
    atomic::{fence, AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time;

pub struct Data {
    data: Box<[u8]>,
    idx: usize,
    atomic_idx: AtomicUsize,
    mutex_idx: Mutex<usize>,
    atomic_bool: Box<[AtomicBool]>,
}

impl Data {
    fn new(l: usize) -> Self {
        let mut data = Vec::new();
        data.resize(l, 0u8);

        let mut atomic_bool = Vec::new();
        atomic_bool.resize_with(l, || AtomicBool::new(false));
        println!("data {}", data.len());
        Data {
            data: data.into_boxed_slice(),
            idx: 0,
            atomic_idx: AtomicUsize::new(0),
            mutex_idx: Mutex::new(0),
            atomic_bool: atomic_bool.into_boxed_slice(),
        }
    }

    fn push(&mut self, data: &[u8]) {
        let now = time::Instant::now();
        loop {
            self.data[self.idx] = data[self.idx % data.len()];
            self.idx += 1;
            if self.idx == self.data.len() {
                break;
            }
        }
        let stop = time::Instant::now();
        println!("{:?}", stop - now);
    }

    fn mutex_push(&mut self, data: &[u8]) {
        let now = time::Instant::now();
        loop {
            let idx = self.idx;
            if idx == self.data.len() {
                break;
            }
            self.data[idx] = data[idx % data.len()];
            let mut guard = self.mutex_idx.lock().unwrap();
            *guard += 1;
            drop(guard);
            self.idx += 1;
        }
        let stop = time::Instant::now();
        println!("{:?}", stop - now);
    }

    fn atomic_seqcst_push(&mut self, data: &[u8]) {
        let now = time::Instant::now();
        loop {
            let idx = self.idx;
            if idx == self.data.len() {
                break;
            }
            self.data[idx] = data[idx % data.len()];
            self.idx = self.atomic_idx.fetch_add(1, Ordering::SeqCst) + 1;
        }
        let stop = time::Instant::now();
        println!("{:?}", stop - now);
    }

    fn optional_atomic_push(&mut self, data: &[u8], sync_freq: usize) {
        let now = time::Instant::now();
        loop {
            let idx = self.idx;
            if idx == self.data.len() {
                break;
            }
            self.data[idx] = data[idx % data.len()];
            fence(Ordering::Release);
            self.idx += 1;
            if idx % sync_freq == 0 {
                self.atomic_idx.store(self.idx, Ordering::SeqCst);
            }
        }
        self.atomic_idx.store(self.idx, Ordering::SeqCst);
        let stop = time::Instant::now();
        println!("{:?}", stop - now);
    }

    fn atomic_fenced_push(&mut self, data: &[u8]) {
        let now = time::Instant::now();
        loop {
            let idx = self.idx;
            if idx == self.data.len() {
                break;
            }
            self.data[idx] = data[idx % data.len()];
            fence(Ordering::Release);
            self.idx = self.atomic_idx.fetch_add(1, Ordering::Relaxed) + 1;
        }
        let stop = time::Instant::now();
        println!("{:?}", stop - now);
    }

    fn atomic_bool_push(&mut self, data: &[u8]) {
        let now = time::Instant::now();
        loop {
            self.data[self.idx] = data[self.idx % data.len()];
            self.atomic_bool[self.idx].store(true, Ordering::SeqCst);
            self.idx += 1;
            if self.idx == self.data.len() {
                break;
            }
        }
        let stop = time::Instant::now();
        println!("{:?}", stop - now);
    }

    fn sum(&self) -> usize {
        let mut d: usize = 0;
        for i in self.data.iter() {
            d += *i as usize;
        }
        d
    }
}

fn run_bool_reader(data: &Data) {
    let l = data.data.len();
    let mut idx = 0;
    loop {
        if data.atomic_bool[idx].load(Ordering::SeqCst) {
            assert!(data.data[idx] > 0);
        }
        idx += 1;
        if idx == l {
            break;
        }
    }
}

fn run_bool_push_read(len: usize, readers: usize) {
    let arr = new_data();
    let data = Arc::new(Data::new(len));
    let data_ref: &mut Data = unsafe { (Arc::as_ptr(&data) as *mut Data).as_mut().unwrap() };
    let mut handles = Vec::new();
    for _ in 0..readers {
        let data_clone = data.clone();
        handles.push(thread::spawn(|| {
            let data = data_clone;
            run_bool_reader(&*data)
        }));
    }
    data_ref.atomic_bool_push(&arr[..]);

    for h in handles {
        h.join().unwrap();
    }
    let d = data.sum();
    println!("{}", d);
}

fn run_fenced_reader(data: &Data) {
    let l = data.data.len();
    loop {
        let idx = data.atomic_idx.load(Ordering::Relaxed);
        fence(Ordering::Acquire);
        if idx > 0 {
            assert!(data.data[idx - 1] > 0);
        }
        if idx == l {
            break;
        }
    }
}

fn run_fenced_push_read(len: usize, readers: usize) {
    let arr = new_data();
    let data = Arc::new(Data::new(len));
    let data_ref: &mut Data = unsafe { (Arc::as_ptr(&data) as *mut Data).as_mut().unwrap() };
    let mut handles = Vec::new();
    for _ in 0..readers {
        let data_clone = data.clone();
        handles.push(thread::spawn(|| {
            let data = data_clone;
            run_fenced_reader(&*data)
        }));
    }
    data_ref.atomic_fenced_push(&arr[..]);

    for h in handles {
        h.join().unwrap();
    }
    let d = data.sum();
    println!("{}", d);
}

fn run_seqcst_reader(data: &Data) {
    let l = data.data.len();
    loop {
        let idx = data.atomic_idx.load(Ordering::SeqCst);
        if idx > 0 {
            assert!(data.data[idx - 1] > 0);
        }
        if idx == l {
            break;
        }
    }
}

fn run_unsynchronized_reader(data: &Data) -> Result<(), &'static str> {
    let l = data.data.len();
    loop {
        if data.idx > 0 && data.data[data.idx - 1] == 0 {
            return Err("Reader runs faster than writer!");
        }
        if data.idx == l {
            return Ok(());
        }
    }
}

fn new_data() -> Box<[u8]> {
    let mut arr = Box::new([0u8; 100]);
    thread_rng().try_fill(&mut arr[..]).unwrap();
    for i in arr.iter_mut() {
        if *i == 0 {
            *i = 1;
        }
    }
    assert!(*arr.iter_mut().min().unwrap() > 0);
    arr
}

fn run_seqcst_push_read(len: usize, readers: usize) {
    let arr = new_data();
    let data = Arc::new(Data::new(len));
    let data_ref: &mut Data = unsafe { (Arc::as_ptr(&data) as *mut Data).as_mut().unwrap() };
    let mut handles = Vec::new();
    for _ in 0..readers {
        let data_clone = data.clone();
        handles.push(thread::spawn(|| {
            let data = data_clone;
            run_seqcst_reader(&*data)
        }));
    }
    data_ref.atomic_seqcst_push(&arr[..]);

    for h in handles {
        h.join().unwrap();
    }
    let d = data.sum();
    println!("{}", d);
}

fn run_optional_sync_push_read(len: usize, readers: usize, sync_freq: usize) {
    let arr = new_data();
    let data = Arc::new(Data::new(len));
    let data_ref: &mut Data = unsafe { (Arc::as_ptr(&data) as *mut Data).as_mut().unwrap() };
    let mut handles = Vec::new();
    for _ in 0..readers {
        let data_clone = data.clone();
        handles.push(thread::spawn(|| {
            let data = data_clone;
            run_unsynchronized_reader(&*data)
        }));
    }
    data_ref.optional_atomic_push(&arr[..], sync_freq);

    for h in handles {
        match h.join() {
            Ok(_) => (),
            Err(e) => {
                panic!("join error: {:?}", e);
            }
        }
    }
    let d = data.sum();
    println!("{}", d);
}

fn run_mutex_reader(data: &Data) {
    let l = data.data.len();
    loop {
        let idx = { *data.mutex_idx.lock().unwrap() };
        if idx > 0 {
            assert!(data.data[idx - 1] > 0);
        }
        if idx == l {
            break;
        }
    }
}

fn run_mutex_push_read(len: usize, readers: usize) {
    let arr = new_data();
    let data = Arc::new(Data::new(len));
    let data_ref: &mut Data = unsafe { (Arc::as_ptr(&data) as *mut Data).as_mut().unwrap() };
    let mut handles = Vec::new();
    for _ in 0..readers {
        let data_clone = data.clone();
        handles.push(thread::spawn(|| {
            let data = data_clone;
            run_mutex_reader(&*data)
        }));
    }
    data_ref.mutex_push(&arr[..]);

    for h in handles {
        h.join().unwrap();
    }
    let d = data.sum();
    println!("{}", d);
}

fn main() {
    //let args: Vec<String> = env::args().collect();
    let len: usize = 10_000_000;

    println!("\nNONE");
    let arr = new_data();
    let mut data = Data::new(len);
    data.push(&arr[..]);
    let d = data.sum();
    println!("{}", d);

    println!("\nMUTEX");
    let arr = new_data();
    let mut data = Data::new(len);
    data.mutex_push(&arr[..]);
    let d = data.sum();
    println!("{}", d);

    println!("\nSEQCST");
    let arr = new_data();
    let mut data = Data::new(len);
    data.atomic_seqcst_push(&arr[..]);
    let d = data.sum();
    println!("{}", d);

    println!("\nRELAXED");
    let arr = new_data();
    let mut data = Data::new(len);
    data.atomic_fenced_push(&arr[..]);
    let d = data.sum();
    println!("{}", d);

    println!("\nATOMIC BOOL ARRAY");
    let arr = new_data();
    let mut data = Data::new(len);
    data.atomic_bool_push(&arr[..]);
    let d = data.sum();
    println!("{}", d);

    println!("\nMUTEX WRITE+READ");
    run_mutex_push_read(len, 4);

    println!("\nSEQCST WRITE+READ");
    run_seqcst_push_read(len, 4);

    println!("\nOPTIONAL SYNC WRITE+READ");
    run_optional_sync_push_read(len, 4, 5);

    println!("\nFENCED WRITE+READ");
    run_fenced_push_read(len, 4);

    println!("\nATOMIC BOOL WRITE+READ");
    run_bool_push_read(len, 4);
}
