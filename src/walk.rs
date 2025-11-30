#![allow(dead_code)]

use std::{collections::VecDeque, iter, marker::PhantomData, ops::ControlFlow};

pub trait Node<T: ?Sized> {
    type Context;

    fn data(&self) -> &T;
    fn data_mut(&mut self) -> &mut T;

    fn children<'a>(&'a self, ctx: &'a Self::Context) -> impl Iterator<Item = &'a Self>;
}

#[derive(Debug)]
pub struct WalkerNode<'a, T, N> {
    pub inner: &'a N,
    pub depth: u64,
    pub sibling_no: u64,
    pub _data: PhantomData<T>,
}

impl<'a, T, N> Clone for WalkerNode<'a, T, N> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T, N> WalkerNode<'a, T, N> {
    fn root(inner: &'a N) -> Self {
        Self {
            inner,
            depth: 0,
            sibling_no: 0,
            _data: PhantomData,
        }
    }
}

impl<'a, T, N> WalkerNode<'a, T, N>
where
    N: Node<T>,
{
    fn children(&self, ctx: &'a N::Context) -> impl Iterator<Item = WalkerNode<'a, T, N>> {
        self.inner
            .children(ctx)
            .enumerate()
            .map(|(i, n)| WalkerNode {
                inner: n,
                depth: self.depth + 1,
                sibling_no: i as u64,
                _data: PhantomData,
            })
    }
}

impl<'a, T, N> Copy for WalkerNode<'a, T, N> {}

#[derive(Clone, Debug)]
pub struct Walker<'a, T, N: Node<T>> {
    ctx: &'a N::Context,
    // A "workhorse" collection: https://nnethercote.github.io/perf-book/heap-allocations.html#reusing-collections
    heap: VecDeque<WalkerNode<'a, T, N>>,
    _data: PhantomData<T>,
}

#[derive(Eq, PartialEq, Default, Copy, Clone, Debug)]
pub enum ContinueFlow {
    #[default]
    Forward,
    Skip,
}

impl<'a, T, N: Node<T>> Walker<'a, T, N> {
    pub fn new(root: &'a N, ctx: &'a N::Context) -> Self {
        Self {
            ctx,
            heap: iter::once(WalkerNode::root(root)).collect(),
            _data: PhantomData,
        }
    }

    pub fn with_capacity(root: &'a N, ctx: &'a N::Context, capacity: usize) -> Self {
        let mut heap = VecDeque::with_capacity(capacity);
        heap.push_front(WalkerNode::root(root));

        Self {
            ctx,
            heap,
            _data: PhantomData,
        }
    }

    pub fn set(&mut self, root: &'a N) {
        self.heap.clear();
        self.heap.push_front(WalkerNode::root(root));
    }
}

impl<'a, T, N: Node<T>> Walker<'a, T, N> {
    pub fn bfs_step<R>(
        &mut self,
        mut f: impl FnMut(WalkerNode<'a, T, N>) -> ControlFlow<R, ContinueFlow>,
    ) -> ControlFlow<R, ContinueFlow> {
        self.bfs_step_by_ref(&mut f)
    }

    pub fn bfs_step_by_ref<R>(
        &mut self,
        f: &mut impl FnMut(WalkerNode<'a, T, N>) -> ControlFlow<R, ContinueFlow>,
    ) -> ControlFlow<R, ContinueFlow> {
        let Some(current_node) = self.heap.pop_front() else {
            return ControlFlow::Continue(ContinueFlow::Forward);
        };

        let control_flow = f(current_node);
        if !matches!(control_flow, ControlFlow::Continue(ContinueFlow::Skip)) {
            self.heap.extend(current_node.children(self.ctx));
        }
        control_flow
    }

    pub fn bfs<R>(
        &mut self,
        mut f: impl FnMut(WalkerNode<'a, T, N>) -> ControlFlow<R, ContinueFlow>,
    ) -> Option<R> {
        while !self.heap.is_empty() {
            if let ControlFlow::Break(value) = self.bfs_step_by_ref(&mut f) {
                return value.into();
            }
        }

        None
    }

    pub fn dfs_step<R>(
        &mut self,
        mut f: impl FnMut(WalkerNode<'a, T, N>) -> ControlFlow<R, ContinueFlow>,
    ) -> ControlFlow<R, ContinueFlow> {
        self.dfs_step_by_ref(&mut f)
    }

    pub fn dfs_step_by_ref<R>(
        &mut self,
        f: &mut impl FnMut(WalkerNode<'a, T, N>) -> ControlFlow<R, ContinueFlow>,
    ) -> ControlFlow<R, ContinueFlow> {
        let Some(current_node) = self.heap.pop_back() else {
            return ControlFlow::Continue(ContinueFlow::Forward);
        };

        let control_flow = f(current_node);
        if !matches!(control_flow, ControlFlow::Continue(ContinueFlow::Skip)) {
            self.heap.extend(current_node.children(self.ctx));
        }
        control_flow
    }

    pub fn dfs<R>(
        &mut self,
        mut f: impl FnMut(WalkerNode<'a, T, N>) -> ControlFlow<R, ContinueFlow>,
    ) -> Option<R> {
        while !self.heap.is_empty() {
            if let ControlFlow::Break(value) = self.dfs_step_by_ref(&mut f) {
                return value.into();
            }
        }

        None
    }
}
