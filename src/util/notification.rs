use std::{cell::Cell, pin::Pin};

use lilos_list::List;
use pin_project::pin_project;

#[derive(Debug)]
#[pin_project]
pub struct Notification<T: Copy> {
    value: Cell<T>,
    #[pin]
    wakers: List<()>,
}

impl<T: Copy> Notification<T> {
    pub const fn new(value: T) -> Self {
        Self {
            value: Cell::new(value),
            wakers: List::new(),
        }
    }

    pub fn send(self: Pin<&Self>, value: T) {
        let this = self.project_ref();
        this.value.set(value);
        this.wakers.wake_all();
    }

    pub async fn receive(self: Pin<&Self>) -> T {
        let this = self.project_ref();
        this.wakers.join(()).await;
        this.value.get()
    }
}
