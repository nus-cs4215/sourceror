use crate::error::DepError;
use crate::error::FetcherError;
use crate::error::GraphError;
use crate::import_name_resolver;
use projstd::log::CompileMessage;
use projstd::log::SourceLocationRef as plSLRef;
use std::boxed::Box;
use std::collections::HashMap;
use std::future::Future;
use std::iter::DoubleEndedIterator;
use std::iter::ExactSizeIterator;
use std::iter::FusedIterator;
use std::pin::Pin;
use std::result::Result;

//#[async_trait(?Send)]
pub trait Fetcher<T>: Copy {
    fn fetch<'a>(
        self,
        name: &'a str,
        sl: plSLRef<'a>,
    ) -> std::pin::Pin<Box<dyn 'a + Future<Output = Result<T, CompileMessage<FetcherError>>>>>;
}

pub trait ExtractDeps<'a> {
    type Iter: Iterator<Item = (import_name_resolver::ResolveIter, plSLRef<'a>)>;
    // Note: If we find a relative import path, then this function should resolve it to a (unique) absolute path before returning.
    // This is because the caller might do caching - to avoid making multiple web requests for the same file,
    // and we will still want to fetch a second file with the same relative path, if the absolute path is different.
    fn extract_deps(&'a self, filename: Option<&'a str>) -> Self::Iter;
}

struct GraphNode<T> {
    deps: Vec<usize>, // indices into Graph::nodes
    content: T,
    name: Option<String>,
}

pub struct Graph<T> {
    nodes: Vec<GraphNode<T>>,
}

impl<T> Graph<T>
where
    for<'a> T: ExtractDeps<'a>,
{
    // Will ensure that nodes with larger index will only depend on nodes with smaller index
    // So the largest index will be the given `t` (root)
    pub async fn try_async_build_from_root<'c, F: Fetcher<T>>(
        t: T,
        f: F,
    ) -> Result<Self, CompileMessage<DepError>> {
        let mut graph = Graph::<T> { nodes: Vec::new() };
        // cache.get(name) == None: never seen this file before
        // cache.get(name) == Some(None): seen this file on the ancestor chain
        // cache.get(name) == Some(Some(idx)): seen this file on an unrelated chain, so it would have already gotten an index
        let mut cache = HashMap::<String, Option<usize>>::new();
        let mut deps = Vec::new();
        for (dep, sl) in t.extract_deps(None) {
            deps.push(
                graph
                    .get_or_fetch_node_recursive(dep, sl, &mut cache, f)
                    .await?,
            );
        }
        // let idx = graph.nodes.len();
        graph.nodes.push(GraphNode {
            deps: deps,
            content: t,
            name: None,
        });
        Ok(graph)
    }

    fn get_or_fetch_node_recursive<'b, F: 'b + Fetcher<T>>(
        &'b mut self,
        mut candidate_resolved_names: impl Iterator<Item = String> + 'static,
        sl: plSLRef<'b>,
        cache: &'b mut HashMap<String, Option<usize>>,
        f: F,
    ) -> Pin<Box<dyn 'b + Future<Output = Result<usize, CompileMessage<DepError>>>>> {
        Box::pin(async move {
            let mut err: Option<CompileMessage<DepError>> = None;
            while let Some(name) = candidate_resolved_names.next() {
                if let Some(opt_idx) = cache.get(name.as_str()) {
                    if let Some(idx) = opt_idx {
                        return Ok(*idx);
                    }
                    return Err(CompileMessage::new_error(sl.to_owned(), GraphError {}).into_cm());
                }
                match f.fetch(name.as_str(), sl).await {
                    Err(e) => {
                        // If we get an error, it could be that the file does not exist (in which case we might get served a custom 404 page)
                        // if that happens, we will get a ESTreeParseError.
                        // So we only continue if we get FetchError or ESTreeParseError (but not ImportsParseError).
                        match e.message() {
                            FetcherError::FetchError(_) | FetcherError::ESTreeParseError(_) => {
                                err = Some(e.into_cm());
                            }
                            _ => {
                                return Err(e.into_cm());
                            }
                        }
                    }
                    Ok(t) => {
                        cache.insert(name.to_owned(), None);
                        let mut deps = Vec::new();
                        for (dep, sl) in t.extract_deps(Some(name.as_str())) {
                            deps.push(self.get_or_fetch_node_recursive(dep, sl, cache, f).await?);
                        }
                        let idx = self.nodes.len();
                        self.nodes.push(GraphNode {
                            deps: deps,
                            content: t,
                            name: Some(name.to_owned()),
                        });
                        *cache.get_mut(name.as_str()).unwrap() = Some(idx);
                        return Ok(idx);
                    }
                }
            }
            // should not panic, because we will definitely have at least one candidate
            Err(err.unwrap())
        })
    }
}

impl<T> Graph<T> {
    // Returns an iterator that traverses the dependency tree in a valid topological ordering.
    // When traversing file A, all dependencies of A must have already been traversed, and the state returned by each of its dependencies will be provided (immutably since there might be diamonds)
    pub fn topological_traverse(
        &self,
    ) -> impl DoubleEndedIterator<Item = (&T, Option<&str>)>
           + ExactSizeIterator<Item = (&T, Option<&str>)>
           + FusedIterator<Item = (&T, Option<&str>)> {
        self.nodes
            .iter()
            .map(|node| (&node.content, node.name.as_deref()))
    }
    pub fn topological_traverse_state_into<
        S,
        E,
        F: FnMut(usize, Box<[&S]>, T, Option<String>) -> Result<S, E>,
    >(
        self,
        mut f: F,
    ) -> Result<(), E> {
        let mut states: Vec<S> = Vec::new();
        states.reserve(self.nodes.len());
        for (i, node) in self.nodes.into_iter().enumerate() {
            let depstates: Box<[&S]> = node.deps.into_iter().map(|x| &states[x]).collect();
            #[allow(mutable_borrow_reservation_conflict)]
            states.push(f(i, depstates, node.content, node.name)?);
        }
        Ok(())
    }

    /*
    pub fn topological_traverse<S: Clone + Combine>(
        &self,
        state: &S,
    ) -> impl DoubleEndedIterator<Item = (&[usize], &T, &str)>
           + ExactSizeIterator<Item = (&[usize], &T, &str)>
           + FusedIterator<Item = (&[usize], &T, &str)> {
        self.nodes
            .iter()
            .map(|node| (node.deps.as_slice(), &node.content, node.name.as_str()))
    }
    pub fn topological_traverse_into<S: Clone + Combine>(
        self,
        state: &S,
    ) -> impl DoubleEndedIterator<Item = (Vec<usize>, T, String)>
           + ExactSizeIterator<Item = (Vec<usize>, T, String)>
           + FusedIterator<Item = (Vec<usize>, T, String)> {
        self.nodes
            .into_iter()
            .map(|node| (node.deps, node.content, node.name))
    }
    */
}
