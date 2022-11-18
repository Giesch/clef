use std::collections::VecDeque;
use std::fmt::Debug;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Queue<T>
where
    T: Debug + Clone + PartialEq + Eq,
{
    pub previous: Vec<T>,
    pub current: T,
    pub next: VecDeque<T>,
}

impl<T> Queue<T>
where
    T: Debug + Clone + PartialEq + Eq,
{
    pub fn try_forward(mut self) -> Result<Self, Self> {
        match self.next.pop_front() {
            Some(new_current) => {
                let new_queue = Queue {
                    previous: {
                        self.previous.push(self.current);
                        self.previous
                    },
                    current: new_current,
                    next: self.next,
                };

                Ok(new_queue)
            }

            None => Err(self),
        }
    }

    pub fn try_back(mut self) -> Result<Self, Self> {
        match self.previous.pop() {
            Some(new_current) => {
                let new_queue = Queue {
                    previous: self.previous,
                    current: new_current,
                    next: {
                        self.next.push_front(self.current);
                        self.next
                    },
                };

                Ok(new_queue)
            }

            None => Err(self),
        }
    }
}
