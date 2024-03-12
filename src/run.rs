//! Utilities to start a CCP processing worker.

use crate::ipc::Backend;
use crate::ipc::BackendBuilder;
use crate::ipc::Ipc;
use crate::lang::Scope;
use crate::serialize;
use crate::serialize::Msg;
use crate::{lang, CongAlg, Datapath, DatapathInfo, Error, Flow, Report, Result};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{atomic, Arc};
use std::thread;
use tracing::{debug, info};

/// A handle to manage running instances of the CCP execution loop.
#[derive(Debug)]
pub struct CCPHandle {
    pub continue_listening: Arc<atomic::AtomicBool>,
    pub join_handle: thread::JoinHandle<Result<()>>,
}

impl CCPHandle {
    /// Instruct the execution loop to exit.
    pub fn kill(&self) {
        self.continue_listening
            .store(false, atomic::Ordering::SeqCst);
    }

    // TODO: join_handle.join() returns an Err instead of Ok, because
    // some function panicked, this function should return an error
    // with the same string from the panic.
    /// Collect the error from the thread running the CCP execution loop
    /// once it exits.
    pub fn wait(self) -> Result<()> {
        match self.join_handle.join() {
            Ok(r) => r,
            Err(_) => Err(Error(String::from("Call to run_inner panicked"))),
        }
    }
}

mod sealed {
    use crate::{ipc::Ipc, CongAlg, Datapath, DatapathInfo, Flow, Report};
    use std::collections::HashMap;

    pub struct AlgList<Head, Tail> {
        pub head_name: String,
        pub head: Head,
        pub tail: Tail,
    }

    pub struct AlgListNil<H>(pub H);

    pub enum Either<L, R> {
        Left(L),
        Right(R),
    }

    impl<L, R> Flow for Either<L, R>
    where
        L: Flow,
        R: Flow,
    {
        fn on_report(&mut self, sock_id: u32, m: Report) {
            use Either::*;
            match self {
                Left(l) => l.on_report(sock_id, m),
                Right(r) => r.on_report(sock_id, m),
            }
        }

        fn close(&mut self) {
            use Either::*;
            match self {
                Left(l) => l.close(),
                Right(r) => r.close(),
            }
        }
    }

    impl<L, R, I> CongAlg<I> for Either<L, R>
    where
        I: Ipc,
        L: CongAlg<I>,
        R: CongAlg<I>,
    {
        type Flow = Either<L::Flow, R::Flow>;

        fn name() -> &'static str {
            ""
        }

        fn datapath_programs(&self) -> HashMap<&'static str, String> {
            use Either::*;
            match self {
                Left(l) => l.datapath_programs(),
                Right(r) => r.datapath_programs(),
            }
        }

        fn new_flow(&self, control: Datapath<I>, info: DatapathInfo) -> Self::Flow {
            use Either::*;
            match self {
                Left(l) => Left(l.new_flow(control, info)),
                Right(r) => Right(r.new_flow(control, info)),
            }
        }
    }

    impl<T, I> CongAlg<I> for &T
    where
        I: Ipc,
        T: CongAlg<I>,
    {
        type Flow = T::Flow;

        fn name() -> &'static str {
            T::name()
        }

        fn datapath_programs(&self) -> HashMap<&'static str, String> {
            T::datapath_programs(self)
        }

        fn new_flow(&self, control: Datapath<I>, info: DatapathInfo) -> Self::Flow {
            T::new_flow(self, control, info)
        }
    }

    pub trait Pick<'a, I: Ipc> {
        type Picked: CongAlg<I> + 'a;
        fn pick(&'a self, name: &str) -> Self::Picked;
    }

    impl<'a, I: Ipc, T: CongAlg<I> + 'a> Pick<'a, I> for AlgListNil<T> {
        type Picked = &'a T;
        fn pick(&'a self, _: &str) -> Self::Picked {
            &self.0
        }
    }

    impl<'a, I: Ipc, T: CongAlg<I> + 'a> Pick<'a, I> for &'a AlgListNil<T> {
        type Picked = &'a T;
        fn pick(&'a self, _: &str) -> Self::Picked {
            &self.0
        }
    }

    impl<'a, I: Ipc, T: CongAlg<I> + 'a, U> Pick<'a, I> for AlgList<Option<T>, U>
    where
        U: Pick<'a, I> + 'a,
        <U as Pick<'a, I>>::Picked: 'a,
    {
        type Picked = Either<&'a T, <U as Pick<'a, I>>::Picked>;
        fn pick(&'a self, name: &str) -> Self::Picked {
            match self.head {
                Some(ref head) if self.head_name == name => Either::Left(&head),
                _ => Either::Right(self.tail.pick(name)),
            }
        }
    }

    impl<'a, I: Ipc, T: CongAlg<I> + 'a, U> Pick<'a, I> for &'a AlgList<Option<T>, U>
    where
        U: Pick<'a, I> + 'a,
        <U as Pick<'a, I>>::Picked: 'a,
    {
        type Picked = Either<&'a T, <U as Pick<'a, I>>::Picked>;
        fn pick(&'a self, name: &str) -> Self::Picked {
            match self.head {
                Some(ref head) if self.head_name == name => Either::Left(&head),
                _ => Either::Right(self.tail.pick(name)),
            }
        }
    }

    pub trait CollectDps<I> {
        fn datapath_programs(&self) -> HashMap<&'static str, String>;
    }

    impl<I: Ipc, T> CollectDps<I> for AlgListNil<T>
    where
        T: CongAlg<I>,
    {
        fn datapath_programs(&self) -> HashMap<&'static str, String> {
            self.0.datapath_programs()
        }
    }

    impl<'a, I: Ipc, T> CollectDps<I> for &'a AlgListNil<T>
    where
        T: CongAlg<I>,
    {
        fn datapath_programs(&self) -> HashMap<&'static str, String> {
            self.0.datapath_programs()
        }
    }

    impl<H, T, I> CollectDps<I> for AlgList<Option<H>, T>
    where
        I: Ipc,
        H: CongAlg<I>,
        T: CollectDps<I>,
    {
        fn datapath_programs(&self) -> HashMap<&'static str, String> {
            self.head
                .iter()
                .flat_map(|x| x.datapath_programs())
                .chain(self.tail.datapath_programs().into_iter())
                .collect()
        }
    }

    impl<'a, H, T, I> CollectDps<I> for &'a AlgList<Option<H>, T>
    where
        I: Ipc,
        H: CongAlg<I>,
        T: CollectDps<I>,
    {
        fn datapath_programs(&self) -> HashMap<&'static str, String> {
            self.head
                .iter()
                .flat_map(|x| x.datapath_programs())
                .chain(self.tail.datapath_programs().into_iter())
                .collect()
        }
    }
}

use sealed::*;

/// Main execution loop of CCP for the static pipeline use case.
/// The `run` method blocks 'forever'; it only returns in two cases:
/// 1. The IPC socket is closed.
/// 2. An invalid message is received.
///
/// Callers must construct a `BackendBuilder`.
/// Algorithm implementations should
/// 1. Initializes an ipc backendbuilder (depending on the datapath).
/// 2. Calls `run()`, or `spawn() `passing the `BackendBuilder b`.
/// Run() or spawn() create Arc<AtomicBool> objects,
/// which are passed into run_inner to build the backend, so spawn() can create a CCPHandle that references this
/// boolean to kill the thread.
///
/// # Example
///
/// Configuration:
/// ```rust,no_run
/// use std::collections::HashMap;
/// use portus::{CongAlg, Flow, Datapath, DatapathInfo, DatapathTrait, Report};
/// use portus::ipc::Ipc;
/// use portus::lang::Scope;
/// use portus::lang::Bin;
/// use portus::RunBuilder;
/// use portus::ipc::{BackendBuilder};
///
/// const PROG: &str = "
///       (def (Report
///           (volatile minrtt +infinity)
///       ))
///       (when true
///           (:= Report.minrtt (min Report.minrtt Flow.rtt_sample_us))
///       )
///       (when (> Micros 42000)
///           (report)
///           (reset)
///       )";
///
/// #[derive(Clone, Default)]
/// struct AlgOne(Scope);
/// impl<I: Ipc> CongAlg<I> for AlgOne {
///     type Flow = Self;
///
///     fn name() -> &'static str {
///         "Default Alg"
///     }
///     fn datapath_programs(&self) -> HashMap<&'static str, String> {
///         let mut h = HashMap::default();
///         h.insert("MyProgram", PROG.to_owned());
///         h
///     }
///     fn new_flow(&self, mut control: Datapath<I>, info: DatapathInfo) -> Self::Flow {
///         let sc = control.set_program("MyProgram", None).unwrap();
///         AlgOne(sc)
///     }
/// }
///
/// impl Flow for AlgOne {
///     fn on_report(&mut self, sock_id: u32, m: Report) {
///         println!("alg1 minrtt: {:?}", m.get_field("Report.minrtt", &self.0).unwrap());
///     }
/// }
///
/// #[derive(Clone, Default)]
/// struct AlgTwo(Scope);
/// impl<I: Ipc> CongAlg<I> for AlgTwo {
///     type Flow = Self;
///
///     fn name() -> &'static str {
///         "Alg2"
///     }
///     fn datapath_programs(&self) -> HashMap<&'static str, String> {
///         let mut h = HashMap::default();
///         h.insert("MyProgram", PROG.to_owned());
///         h
///     }
///     fn new_flow(&self, mut control: Datapath<I>, info: DatapathInfo) -> Self::Flow {
///         let sc = control.set_program("MyProgram", None).unwrap();
///         AlgTwo(sc)
///     }
/// }
///
/// impl Flow for AlgTwo {
///     fn on_report(&mut self, sock_id: u32, m: Report) {
///         println!("alg2 minrtt: {:?}", m.get_field("Report.minrtt", &self.0).unwrap());
///     }
/// }
///
/// fn main() {
///   let b = portus::ipc::unix::Socket::<portus::ipc::Blocking>::new("portus").map(|sk| BackendBuilder { sock: sk }).expect("ipc initialization");
///   let rb = RunBuilder::new(b)
///     .default_alg(AlgOne::default())
///     .additional_alg(AlgTwo::default())
///     .additional_alg::<AlgOne, _>(None);
///     // .spawn_thread() to spawn runtime in a thread
///     // .with_stop_handle() to pass in an Arc<AtomicBool> that will stop the runtime
///   rb.run();
/// }
/// ```
pub struct RunBuilder<I: Ipc, U, Spawnness> {
    backend_builder: BackendBuilder<I>,
    alg: U,
    stop_handle: Option<*const atomic::AtomicBool>,
    _phantom: std::marker::PhantomData<Spawnness>,
}

pub struct Spawn;
pub struct NoSpawn;

impl<I: Ipc> RunBuilder<I, (), NoSpawn> {
    pub fn new(backend_builder: BackendBuilder<I>) -> Self {
        Self {
            backend_builder,
            alg: (),
            stop_handle: None,
            _phantom: Default::default(),
        }
    }
}

impl<I: Ipc, S> RunBuilder<I, (), S> {
    /// Set the default congestion control algorithm. This is required to run or spawn anything.
    ///
    /// This is the algorithm that will be used if the name the datapath requests doesn't match
    /// anything.
    pub fn default_alg<A>(self, alg: A) -> RunBuilder<I, AlgListNil<A>, S> {
        RunBuilder {
            alg: AlgListNil(alg),
            backend_builder: self.backend_builder,
            stop_handle: self.stop_handle,
            _phantom: Default::default(),
        }
    }
}

impl<I: Ipc, U, S> RunBuilder<I, U, S> {
    /// Set an additional congestion control algorithm.
    ///
    /// If the name duplicates one already given, the later one will win.
    pub fn additional_alg<A: CongAlg<I>, O: Into<Option<A>>>(
        self,
        alg: O,
    ) -> RunBuilder<I, AlgList<Option<A>, U>, S> {
        RunBuilder {
            alg: AlgList {
                head_name: A::name().to_owned(),
                head: alg.into(),
                tail: self.alg,
            },
            backend_builder: self.backend_builder,
            stop_handle: self.stop_handle,
            _phantom: Default::default(),
        }
    }

    pub fn try_additional_alg<A: CongAlg<I>>(
        self,
        alg: Option<A>,
    ) -> RunBuilder<I, AlgList<Option<A>, U>, S> {
        RunBuilder {
            alg: AlgList {
                head_name: A::name().to_owned(),
                head: alg,
                tail: self.alg,
            },
            backend_builder: self.backend_builder,
            stop_handle: self.stop_handle,
            _phantom: Default::default(),
        }
    }

    /// Pass an `AtomicBool` stop handle.
    pub fn with_stop_handle(self, handle: Arc<atomic::AtomicBool>) -> Self {
        Self {
            stop_handle: Some(Arc::into_raw(handle)),
            ..self
        }
    }

    /// Pass a raw pointer to an `AtomicBool` stop handle.
    ///
    /// # Safety
    /// `handle_ptr` must be from
    /// [`Arc::into_raw()`](https://doc.rust-lang.org/std/sync/struct.Arc.html#method.from_raw).
    // this is unsafe so that we can safely use unsafe blocks when actually running: we need to
    // pass the unsafe parcel to the caller, since we can't guarantee safety.
    pub unsafe fn with_raw_stop_handle(self, handle_ptr: *const atomic::AtomicBool) -> Self {
        Self {
            stop_handle: Some(handle_ptr),
            ..self
        }
    }

    fn stop_handle(&self) -> Result<Arc<atomic::AtomicBool>> {
        if let Some(ptr) = self.stop_handle {
            if ptr.is_null() {
                return Err(Error(String::from("handle is null")));
            }

            Ok(unsafe { Arc::from_raw(ptr) })
        } else {
            Ok(Arc::new(atomic::AtomicBool::new(true)))
        }
    }
}

impl<I: Ipc, U> RunBuilder<I, U, NoSpawn> {
    /// Spawn a thread which will perform the CCP execution loop. Returns
    /// a `CCPHandle`, which the caller can use to cause the execution loop
    /// to stop.
    /// The `run` method blocks 'forever'; it only returns in three cases:
    /// 1. The IPC socket is closed.
    /// 2. An invalid message is received.
    /// 3. The caller calls `CCPHandle::kill()`
    ///
    /// See [`run`](./fn.run.html) for more information.
    pub fn spawn_thread(self) -> RunBuilder<I, U, Spawn> {
        RunBuilder {
            backend_builder: self.backend_builder,
            stop_handle: self.stop_handle,
            alg: self.alg,
            _phantom: Default::default(),
        }
    }
}

impl<I, U> RunBuilder<I, U, NoSpawn>
where
    I: Ipc,
    for<'a> &'a U: Pick<'a, I> + CollectDps<I>,
{
    pub fn run(self) -> Result<()> {
        let h = self.stop_handle()?;
        run_inner(h, self.backend_builder, self.alg)
    }
}

impl<I, U> RunBuilder<I, U, Spawn>
where
    I: Ipc,
    U: Send + 'static,
    for<'a> &'a U: Pick<'a, I> + CollectDps<I>,
{
    pub fn run(self) -> Result<CCPHandle> {
        let stop_signal = self.stop_handle()?;
        let bb = self.backend_builder;
        let alg = self.alg;
        Ok(CCPHandle {
            continue_listening: stop_signal.clone(),
            join_handle: thread::spawn(move || run_inner(stop_signal, bb, alg)),
        })
    }
}

// Main execution inner loop of ccp.
// Blocks "forever", or until the iterator stops iterating.
//
// `run_inner()`:
// 1. listens for messages from the datapath
// 2. call the appropriate message in `U: impl CongAlg`
// The function can return for two reasons: an error, or the iterator returned None.
// The latter should only happen for spawn(), and not for run().
// It returns any error, either from:
// 1. the IPC channel failing
// 2. Receiving an install control message (only the datapath should receive these).
fn run_inner<I, U>(
    continue_listening: Arc<atomic::AtomicBool>,
    backend_builder: BackendBuilder<I>,
    algs: U,
) -> Result<()>
where
    I: Ipc,
    for<'a> &'a U: Pick<'a, I> + CollectDps<I>,
{
    let mut receive_buf = [0u8; 1024];
    let mut b = backend_builder.build(continue_listening.clone(), &mut receive_buf[..]);
    // the borrow has to before the HashMap, to guarantee that the HashMap is dropped first
    let algs2 = &algs;
    let mut dp_to_flowmap = HashMap::<
        I::Addr,
        HashMap<u32, <<&'_ U as Pick<'_, I>>::Picked as CongAlg<I>>::Flow>,
    >::new();

    info!(ipc = ?I::name(), "starting CCP");

    let mut scope_map = Rc::new(HashMap::<String, Scope>::default());
    let mut install_msgs = vec![];

    let programs = algs2.datapath_programs();
    for (program_name, program) in programs.iter() {
        match lang::compile(program.as_bytes(), &[]) {
            Ok((bin, sc)) => {
                let msg = serialize::install::Msg {
                    sid: 0,
                    program_uid: sc.program_uid,
                    num_events: bin.events.len() as u32,
                    num_instrs: bin.instrs.len() as u32,
                    instrs: bin,
                };
                let buf = serialize::serialize(&msg)?;
                install_msgs.push(buf);

                Rc::get_mut(&mut scope_map)
                    .unwrap()
                    .insert(program_name.to_string(), sc.clone());
            }
            Err(e) => {
                return Err(Error(format!(
                    "Datapath program \"{}\" failed to compile: {:?}",
                    program_name, e
                )));
            }
        }
    }

    fn maybe_try_install_datapath_programs<I, A>(
        backend: &Backend<'_, I>,
        install_msgs: &[Vec<u8>],
    ) -> Result<()>
    where
        I: Ipc<Addr = A>,
        A: Default + 'static,
    {
        let a = A::default();
        let x: &dyn std::any::Any = &a;
        if x.downcast_ref::<()>().is_some() {
            info!("detected address-less datapath, installing programs");
            let s = backend.sender(a);
            for buf in install_msgs {
                s.send_msg(&buf[..])?;
            }

            Ok(())
        } else {
            Ok(())
        }
    }

    debug!(programs = %format!("{:#?}", programs.keys()), "compiled all datapath programs, ccp ready");
    maybe_try_install_datapath_programs(&b, &install_msgs)?;
    while let Some((msg, recv_addr)) = b.next() {
        match msg {
            Msg::Rdy(_r) => {
                if dp_to_flowmap.remove(&recv_addr).is_some() {
                    info!(
                        "new ready from old datapath, clearing old flows and installing programs"
                    );
                } else {
                    info!(addr = %format!("{:#?}", recv_addr), "found new datapath, installing programs");
                }

                dp_to_flowmap.insert(
                    recv_addr.clone(),
                    HashMap::<u32, <<&'_ U as Pick<'_, I>>::Picked as CongAlg<I>>::Flow>::default(),
                );

                let backend = b.sender(recv_addr);
                for buf in &install_msgs {
                    backend.send_msg(&buf[..])?;
                }
            }
            Msg::Cr(c) => {
                let mut need_install = false;
                let flowmap = dp_to_flowmap.entry(recv_addr.clone()).or_insert_with_key(|recv_addr| {
                    debug!(addr = %format!("{:#?}", recv_addr), "received create from unknown datapath, initializing");
                    need_install = true;
                    HashMap::<u32, <<&'_ U as Pick<'_, I>>::Picked as CongAlg<I>>::Flow>::default()
                });

                if need_install {
                    debug!(addr = %format!("{:#?}", recv_addr), "installing programs");
                    let backend = b.sender(recv_addr.clone());
                    for buf in &install_msgs {
                        backend.send_msg(&buf[..])?;
                    }
                }

                if flowmap.remove(&c.sid).is_some() {
                    debug!(sid = ?c.sid, "re-creating already created flow");
                }

                debug!(
                    sid        = ?c.sid,
                    init_cwnd  = ?c.init_cwnd,
                    mss        = ?c.mss,
                    src_ip     = ?c.src_ip,
                    src_port   = ?c.src_port,
                    dst_ip     = ?c.dst_ip,
                    dst_port   = ?c.dst_port,
                    alg        = ?c.cong_alg.as_ref(),
                    "creating new flow"
                );

                let alg = algs2.pick(c.cong_alg.as_ref().map(String::as_str).unwrap_or(""));
                let f = alg.new_flow(
                    Datapath {
                        sock_id: c.sid,
                        sender: b.sender(recv_addr),
                        programs: scope_map.clone(),
                    },
                    DatapathInfo {
                        sock_id: c.sid,
                        init_cwnd: c.init_cwnd,
                        mss: c.mss,
                        src_ip: c.src_ip,
                        src_port: c.src_port,
                        dst_ip: c.dst_ip,
                        dst_port: c.dst_port,
                    },
                );
                flowmap.insert(c.sid, f);
            }
            Msg::Ms(m) => {
                let flowmap = match dp_to_flowmap.get_mut(&recv_addr) {
                    Some(fm) => fm,
                    None => {
                        info!(addr = %format!("{:#?}", recv_addr), "received measurement from unknown datapath, ignoring");
                        continue;
                    }
                };

                if flowmap.contains_key(&m.sid) {
                    if m.num_fields == 0 {
                        let mut flow = flowmap.remove(&m.sid).unwrap();
                        flow.close();
                    } else {
                        let flow = flowmap.get_mut(&m.sid).unwrap();
                        flow.on_report(
                            m.sid,
                            Report {
                                program_uid: m.program_uid,
                                from: format!("{:#?}", recv_addr),
                                fields: m.fields,
                            },
                        )
                    }
                } else {
                    debug!(sid = m.sid, "measurement for unknown flow");
                }
            }
            Msg::Ins(_) => {
                // The start() listener should never receive an install message, since it is on the CCP side.
                unreachable!()
            }
            Msg::Other(m) => {
                debug!(
                    size = ?m.len,
                    msg_type = ?m.typ,
                    sid = ?m.sid,
                    addr = %format!("{:#?}", recv_addr),
                    "got unknown message"
                );
                continue;
            }
        }
    }

    // if the thread has been killed, return that as error
    if !continue_listening.load(atomic::Ordering::SeqCst) {
        info!("portus shutting down");
        Ok(())
    } else {
        Err(Error(String::from("The IPC channel has closed.")))
    }
}
