use std::{
    pin::Pin,
    sync::{
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc, Mutex,
    },
    task::{Context, Poll},
};

use futures::{
    future::BoxFuture,
    task::{waker_ref, ArcWake, Spawn},
    Future, FutureExt,
};

// スケジューリングの対象となる計算の実行単位
struct Task {
    // fire and for get
    // Q: Task にMutexが必要な理由は？一つのタスクを複数が見ることはなさそう。m
    future: Mutex<BoxFuture<'static, ()>>, // trait にasync fnを実装するためにBoxが必要. sized の制約から解かれる
    // 自身でスケジューリングできるようにsenderも持っておく
    sender: SyncSender<Arc<Task>>,
}

// waker を作りたいので ArcWake を実装する必要がある
impl ArcWake for Task {
    // タスクへのArc参照をチャネルんで送ることで、スケジューリングする
    // = executor へ enque する
    // Q: wake って実行キューに詰むという意味であってる？
    // 起動
    fn wake_by_ref(arc_self: &Arc<Self>) {
        let self0 = arc_self.clone();
        arc_self.sender.send(self0).unwrap();
    }
}

struct Executor {
    sender: SyncSender<Arc<Task>>,
    receiver: Receiver<Arc<Task>>,
}

impl Executor {
    fn new() -> Self {
        let (sender, receiver) = sync_channel(1024);
        Executor {
            // Q: new のたびにsender作るからここのclone無駄な気がする
            sender: sender.clone(),
            receiver,
        }
    }

    fn get_spawner(&self) -> Spawner {
        Spawner {
            //借用しているのでclone必要
            sender: self.sender.clone(),
        }
    }

    fn run(&self) {
        while let Ok(task) = self.receiver.recv() {
            let mut future = task.future.lock().unwrap();
            let waker = waker_ref(&task);
            // stackless coroutine: 保持しない、なのでcontextを使う
            // stackfull coroutine: stackを保持する  
            let mut ctx = Context::from_waker(&waker);
            let result = future.as_mut().poll(&mut ctx);
            match result {
                Poll::Ready(_) => break,
                Poll::Pending => todo!(),
            } 
        }
    }
}

struct Spawner {
    sender: SyncSender<Arc<Task>>,
}



impl Spawner {
    // Q: impl Trait が嬉しい理由
    // 戻り値を隠ぺいする仕組み
    // ないとBox<dyn ... を使う必要がある
    // trait object = virtual func => サイズが決まらないからBoxも必要
    fn spawn(&self, future: impl Future<Output = ()> + 'static + Send) {
        let future = future.boxed();
        let task = Arc::new(Task {
            future: Mutex::new(future),
            sender: self.sender.clone(),
        });

        self.sender.send(task).unwrap();
    }
}

enum StateHello {
    HELLO,
    WORLD,
    END,
}

struct Hello {
    state: StateHello,
}

impl Hello {
    fn new() -> Self {
        Hello {
            state: StateHello::HELLO,
        }
    }
}

impl Future for Hello {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // deref は勝手にコンパイラがやってくれる
        match (*self).state {
            StateHello::HELLO => {
                println!("hello");
                // Q: dereference が必要な理由は？
                // 無くても勝手にderefされてるだけ
                (*self).state = StateHello::WORLD;
                cx.waker().wake_by_ref();
                // Q: Pending を返す意味がないと思う。結局ループ回るよね
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
