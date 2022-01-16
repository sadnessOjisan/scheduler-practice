use std::{sync::{Mutex, mpsc::{SyncSender, sync_channel, Receiver}, Arc}, task::{Context, Poll}, pin::Pin};

use futures::{future::BoxFuture, task::{ArcWake, waker_ref, Spawn}, Future, FutureExt};

// スケジューリングの対象となる計算の実行単位
struct Task {
    future: Mutex<BoxFuture<'static, ()>>,
    // 自身でスケジューリングできるようにsenderも持っておく
    sender: SyncSender<Arc<Task>>
}

impl ArcWake for Task {
    // タスクへのArc参照をチャネルんで送ることで、スケジューリングする
    // = executor へ enque する
    fn wake_by_ref(arc_self: &Arc<Self>){
        let self0 = arc_self.clone();
        arc_self.sender.send(self0).unwrap();
    }
}

struct Executor {
    sender: SyncSender<Arc<Task>>,
    receiver: Receiver<Arc<Task>>
}

impl Executor {
    fn new() -> Self {
        let (sender, receiver) = sync_channel(1024);
        Executor {
            // Q: ここにどうしてcloneが必要なのか
            sender: sender.clone(),
            receiver
        }
    }

    fn get_spawner(&self) -> Spawner{
        Spawner {
            sender: self.sender.clone()
        }
    }

    fn run(&self){
        while let Ok(task) = self.receiver.recv(){
            let mut future = task.future.lock().unwrap();
            let waker = waker_ref(&task);
            let mut ctx = Context::from_waker(&waker);


            let _ = future.as_mut().poll(&mut ctx);
        }
    }
}

struct Spawner {
    sender: SyncSender<Arc<Task>>
}

impl Spawner {
    fn spawn(&self, future: impl Future<Output = ()> + 'static + Send){
        let future = future.boxed();
        let task = Arc::new(Task {
            future: Mutex::new(future),
            sender: self.sender.clone()
        });

        self.sender.send(task).unwrap();
    }
}

enum StateHello {
    HELLO,
    WORLD,
    END
}

struct Hello {
    state: StateHello
}

impl Hello {
    fn new() -> Self {
        Hello {
            state: StateHello::HELLO
        }
    }
}

impl Future for Hello {
    type Output = ();
    fn poll (mut self: Pin<&mut Self>, cx: &mut Context<'_>)->Poll<()> {
        match (*self).state{
            StateHello::HELLO => {
                println!("hello");
                (*self).state = StateHello::WORLD;
                cx.waker().wake_by_ref();
               return Poll::Pending;
            }
            StateHello::WORLD => {
                println!("world");
                (*self).state = StateHello::END;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            StateHello::END => {
               return Poll::Ready(());
            }
        }
    }
}

fn main() {
    let executor = Executor::new();
    executor.get_spawner().spawn(Hello::new());
    executor.run();
}
