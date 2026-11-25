use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct LogBuffer {
    inner: Arc<Mutex<VecDeque<String>>>,
    cap: usize,
    tx: broadcast::Sender<String>,
}

impl LogBuffer {
    pub fn new(cap: usize) -> Self {
        let (tx, _) = broadcast::channel(cap.max(16));
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(cap))),
            cap,
            tx,
        }
    }

    pub fn push(&self, line: String) {
        {
            let mut g = self.inner.lock().unwrap();
            if g.len() >= self.cap {
                g.pop_front();
            }
            g.push_back(line.clone());
        }
        let _ = self.tx.send(line);
    }

    pub fn tail(&self, n: usize) -> Vec<String> {
        let g = self.inner.lock().unwrap();
        let start = g.len().saturating_sub(n);
        g.iter().skip(start).cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_caps() {
        let b = LogBuffer::new(3);
        for i in 0..5 {
            b.push(format!("l{}", i));
        }
        assert_eq!(b.len(), 3);
        assert_eq!(b.tail(10), vec!["l2", "l3", "l4"]);
    }

    #[test]
    fn tail_n() {
        let b = LogBuffer::new(10);
        for i in 0..5 {
            b.push(format!("l{}", i));
        }
        assert_eq!(b.tail(2), vec!["l3", "l4"]);
        assert_eq!(b.tail(0), Vec::<String>::new());
    }

    #[test]
    fn subscribe_receives() {
        let b = LogBuffer::new(10);
        let mut rx = b.subscribe();
        b.push("hi".into());
        let v = rx.try_recv().unwrap();
        assert_eq!(v, "hi");
    }
}
