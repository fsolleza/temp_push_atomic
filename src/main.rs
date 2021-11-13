use std::thread;
use std::sync::{Mutex, Arc, atomic::{AtomicUsize, Ordering, fence}};
use std::time;
use std::env;
use rand::thread_rng;
use rand::Rng;

pub struct Data {
    data: Box<[u8]>,
    idx: usize,
    atomic_idx: AtomicUsize,
    mutex_data: Mutex<(Box<[u8]>, usize)>,
}

impl Data {
    fn new(l: usize) -> Self {
        let mut data = Vec::new();
        data.resize(l, 0u8);
        let mutex_data = Mutex::new((data.into_boxed_slice(), 0));

        let mut data = Vec::new();
        data.resize(l, 0u8);
        println!("data {}", data.len());
        Data {
            data: data.into_boxed_slice(),
            idx: 0,
            atomic_idx: AtomicUsize::new(0),
            mutex_data,
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
            let mut guard = self.mutex_data.lock().unwrap();
            let idx = guard.1;
            guard.0[idx] = data[idx % data.len()];
            guard.1 += 1;
            if guard.1 == self.data.len() {
                break;
            }
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

    fn sum(&self) -> usize {
        let mut d: usize = 0;
        for i in self.data.iter() {
            d += *i as usize;
        }
        d
    }

    fn mutex_sum(&self) -> usize {
        let mut d: usize = 0;
        let guard = self.mutex_data.lock().unwrap();
        for i in guard.0.iter() {
            d += *i as usize;
        }
        d
    }
}

fn run_fenced_reader(data: &Data) {
    let l = data.data.len();
    loop {
        let idx = data.atomic_idx.load(Ordering::Relaxed);
        fence(Ordering::Acquire);
        if idx == 0 {
        } else {
            assert!(data.data[idx-1] > 0);
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
        if idx == 0 {
        } else {
            assert!(data.data[idx-1] > 0);
        }
        if idx == l {
            break;
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

fn main() {
    let args: Vec<String> = env::args().collect();
    let len: usize = args[2].parse().unwrap();

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
    let d = data.mutex_sum();
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

    println!("\nSEQCST WRITE+READ");
    run_seqcst_push_read(len, 4);
    println!("\nFENCED WRITE+READ");
    run_fenced_push_read(len, 4);
}
