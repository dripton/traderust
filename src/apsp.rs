use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use rayon::prelude::*;

extern crate bucket_queue;
use bucket_queue::*;

use clap::ArgEnum;

extern crate ndarray;
use ndarray::Array2;

pub const INFINITY: u16 = u16::MAX;
pub const NO_PRED_NODE: u16 = INFINITY - 1;

#[derive(ArgEnum, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Algorithm {
    Dijkstra,
    Dial,
    Floyd,
}

/// Floyd-Warshall is a simple O(V^3) algorithm, where V is the number of
/// vertexes.  We iterate over i, j, and k.  If dist[i, j] > dist[i, k] +
/// dist[k, j] then we set dist[i, j] to that sum, and set pred[i, j] to
/// pred[k, j] to show that the shortest path now runs through that node.  This
/// implementation is currently single-threaded.  Even if it were
/// multi-threaded, Floyd-Warshall is so much slower than Dijkstra for sparse
/// matrixes (E << V^2) that it should not be used except for testing.
fn floyd_warshall(dist: &mut Array2<u16>) -> Array2<u16> {
    let size = dist.nrows();
    let mut pred = Array2::<u16>::from_elem((size, size), NO_PRED_NODE);

    // Set all zero vertexes to infinity
    for i in 0..size {
        for j in 0..size {
            if dist[[i, j]] == 0 {
                dist[[i, j]] = INFINITY;
            }
        }
    }

    // Set each vertex at zero distance to itself
    for i in 0..size {
        dist[[i, i]] = 0;
    }

    // Assume bidirectional movement
    for i in 0..size {
        for j in 0..size {
            if dist[[i, j]] > dist[[j, i]] {
                dist[[i, j]] = dist[[j, i]];
            }
        }
    }

    // Initialize predecessors where we have paths
    for i in 0..size {
        for j in 0..size {
            if dist[[i, j]] > 0 && dist[[i, j]] < INFINITY {
                pred[[i, j]] = i as u16;
            }
        }
    }

    // Do the Floyd Warshall triple nested loop
    for k in 0..size {
        for i in 0..size {
            for j in 0..size {
                if dist[[i, k]] != INFINITY
                    && dist[[k, j]] != INFINITY
                    && dist[[i, j]] > dist[[i, k]] + dist[[k, j]]
                {
                    dist[[i, j]] = dist[[i, k]] + dist[[k, j]];
                    pred[[i, j]] = pred[[k, j]];
                }
            }
        }
    }
    pred
}

fn dijkstra_one_row(
    start: u16,
    size: usize,
    neighbors_map: &HashMap<u16, HashSet<u16>>,
    weights: &HashMap<(u16, u16), u16>,
) -> (Vec<u16>, Vec<u16>) {
    let mut dist_row = vec![INFINITY; size];
    let mut pred_row = vec![NO_PRED_NODE; size];

    // TODO Try a Fibonacci heap instead
    let mut heap = BinaryHeap::new();

    dist_row[start as usize] = 0;
    heap.push(Reverse((0, start)));

    while !heap.is_empty() {
        if let Some(Reverse((priority, u))) = heap.pop() {
            if priority == dist_row[u as usize] as u16 {
                if let Some(neighbors) = neighbors_map.get(&u) {
                    for v in neighbors {
                        let weight = weights.get(&(u, *v)).unwrap();
                        let alt = dist_row[u as usize] as u16 + weight;
                        if alt < (dist_row[*v as usize]) as u16 {
                            dist_row[*v as usize] = alt as u16;
                            pred_row[*v as usize] = u as u16;
                            let tup = (alt, *v);
                            heap.push(Reverse(tup));
                        }
                    }
                }
            }
        }
    }

    (dist_row, pred_row)
}

fn dial_one_row(
    start: u16,
    size: usize,
    neighbors_map: &HashMap<u16, HashSet<u16>>,
    weights: &HashMap<(u16, u16), u16>,
) -> (Vec<u16>, Vec<u16>) {
    let mut dist_row = vec![INFINITY; size];
    let mut pred_row = vec![NO_PRED_NODE; size];

    let mut queue = BucketQueue::<VecDeque<u16>>::new();

    dist_row[start as usize] = 0;
    queue.enqueue(start, 0);

    while !queue.is_empty() {
        if let Some(priority) = queue.min_priority() {
            if let Some(u) = queue.dequeue_min() {
                if priority == dist_row[u as usize] as usize {
                    if let Some(neighbors) = neighbors_map.get(&u) {
                        for v in neighbors {
                            let weight = weights.get(&(u, *v)).unwrap();
                            let alt = dist_row[u as usize] as u16 + weight;
                            if alt < (dist_row[*v as usize]) as u16 {
                                dist_row[*v as usize] = alt as u16;
                                pred_row[*v as usize] = u as u16;
                                queue.enqueue(*v as u16, alt as usize);
                            }
                        }
                    }
                }
            }
        }
    }

    (dist_row, pred_row)
}

fn dijkstra_dial_inner(dist: &mut Array2<u16>, alg: Algorithm) -> Array2<u16> {
    let size = dist.nrows();
    let mut pred = Array2::<u16>::from_elem((size, size), NO_PRED_NODE);

    // Set all zero vertexes to infinity
    for i in 0..size {
        for j in 0..size {
            if dist[[i, j]] == 0 {
                dist[[i, j]] = INFINITY;
            }
        }
    }

    // Set each vertex at zero distance to itself
    for i in 0..size {
        dist[[i, i]] = 0;
    }

    // Assume bidirectional movement
    for i in 0..size {
        for j in 0..size {
            if dist[[i, j]] > dist[[j, i]] {
                dist[[i, j]] = dist[[j, i]];
            }
        }
    }

    // Populate neighbors_map
    let mut neighbors_map: HashMap<u16, HashSet<u16>> = HashMap::new();
    for i in 0..size {
        let set = HashSet::new();
        neighbors_map.insert(i as u16, set);
    }
    for i in 0..size {
        for j in 0..size {
            if dist[[i, j]] > 0 && dist[[i, j]] < INFINITY {
                let set = neighbors_map.get_mut(&(i as u16)).unwrap();
                set.insert(j as u16);
            }
        }
    }

    // Populate weights
    let mut weights: HashMap<(u16, u16), u16> = HashMap::new();
    for i in 0..size {
        for j in 0..size {
            if dist[[i, j]] > 0 && dist[[i, j]] != INFINITY {
                weights.insert((i as u16, j as u16), dist[[i, j]] as u16);
            }
        }
    }

    // Initialize predecessors where we have paths
    for i in 0..size {
        for j in 0..size {
            if dist[[i, j]] > 0 && dist[[i, j]] < INFINITY {
                pred[[i, j]] = i as u16;
            }
        }
    }

    let tuples: Vec<(Vec<u16>, Vec<u16>)>;
    // Do the Dijkstra or Dial algorithm for each row, in parallel using Rayon
    if alg == Algorithm::Dijkstra {
        tuples = (0..size)
            .into_par_iter()
            .map(|i| dijkstra_one_row(i as u16, size, &neighbors_map, &weights))
            .collect();
    } else if alg == Algorithm::Dial {
        tuples = (0..size)
            .into_par_iter()
            .map(|i| dial_one_row(i as u16, size, &neighbors_map, &weights))
            .collect();
    } else {
        panic!("invalid Algorithm");
    }
    for (i, (dist_row, pred_row)) in tuples.iter().enumerate() {
        // TODO Find a way to copy the entire row
        for (j, dist_el) in dist_row.iter().enumerate() {
            dist[[i, j]] = *dist_el;
        }
        for (j, pred_el) in pred_row.iter().enumerate() {
            pred[[i, j]] = *pred_el;
        }
    }

    pred
}

pub fn shortest_path(dist: &mut Array2<u16>, alg: Algorithm) -> Array2<u16> {
    match alg {
        Algorithm::Dial => dial(dist),
        Algorithm::Dijkstra => dijkstra(dist),
        Algorithm::Floyd => floyd_warshall(dist),
    }
}

/// Dijkstra's algorithm for all-pairs shortest path is just Dijkstra from a
/// single source to all destinations, repeated for every source.  That makes
/// it very easy to parallelize by using one thread per starting node.  The
/// basic idea of Dijkstra is to put the start node on a priority queue.  Then,
/// as long as the queue is not empty, pop the minimum node off the queue as
/// node u, look at all of its neighbor nodes as node v, and see if the
/// distance to node v through u is less than the previously recorded distance.
/// If it is, update the distance to v to the new distance and make v's
/// predecessor u, then put v and its current distance on the queue.  This
/// implementation uses Rust's BinaryHeap as the priority queue.  BinaryHeap
/// is a max-heap, so nodes are wrapped in std::cmp::Reverse to make it work
/// as a min-heap.  APSP Dijkstra's runtime for V nodes and E edges, with a
/// binary heap, is O(((E + V) log V)V).
fn dijkstra(dist: &mut Array2<u16>) -> Array2<u16> {
    dijkstra_dial_inner(dist, Algorithm::Dijkstra)
}

/// Dial's algorithm is Dijkstra's algorithm with a bucket queue used for the
/// priority queue.  A bucket queue is basically an array of deques, one per
/// priority, so only works well for a small number of priorities.  In this case
/// all non-trivial edge weights are integers in the range 1 through 4, so a
/// bucket queue with 4 buckets works fine.  A bucket queue has O(1) push and
/// O(buckets) pop, compared to the binary heap's O(log n) push and O(log n)
/// pop.  This makes APSP Dial's runtime O((E + VC)V) for E edges, V nodes, and
/// C distinct edge weights.
fn dial(dist: &mut Array2<u16>) -> Array2<u16> {
    dijkstra_dial_inner(dist, Algorithm::Dial)
}

#[cfg(test)]
mod tests {
    use super::*;

    use log::debug;

    extern crate rand;
    use rand::prelude::*;

    fn setup_scipy_test() -> Array2<u16> {
        // https://docs.scipy.org/doc/scipy/reference/generated/scipy.sparse.csgraph.shortest_path.html
        let mut dist = Array2::<u16>::from_elem((4, 4), INFINITY);
        dist[[0, 1]] = 1;
        dist[[0, 2]] = 2;
        dist[[1, 3]] = 1;
        dist[[2, 0]] = 2;
        dist[[2, 3]] = 3;
        dist[[3, 3]] = 0;
        debug!("dist before {:?}\n", dist);
        dist
    }

    fn compare_scipy_test(dist: Array2<u16>, pred: Array2<u16>) {
        debug!("dist after {:?}\n", dist);
        debug!("pred after {:?}\n", pred);

        assert_eq!(dist[[0, 0]], 0);
        assert_eq!(dist[[0, 1]], 1);
        assert_eq!(dist[[0, 2]], 2);
        assert_eq!(dist[[0, 3]], 2);

        assert_eq!(dist[[1, 0]], 1);
        assert_eq!(dist[[1, 1]], 0);
        assert_eq!(dist[[1, 2]], 3);
        assert_eq!(dist[[1, 3]], 1);

        assert_eq!(dist[[2, 0]], 2);
        assert_eq!(dist[[2, 1]], 3);
        assert_eq!(dist[[2, 2]], 0);
        assert_eq!(dist[[2, 3]], 3);

        assert_eq!(dist[[3, 0]], 2);
        assert_eq!(dist[[3, 1]], 1);
        assert_eq!(dist[[3, 2]], 3);
        assert_eq!(dist[[3, 3]], 0);

        assert_eq!(pred[[0, 0]], NO_PRED_NODE);
        assert_eq!(pred[[0, 1]], 0);
        assert_eq!(pred[[0, 2]], 0);
        assert_eq!(pred[[0, 3]], 1);

        assert_eq!(pred[[1, 0]], 1);
        assert_eq!(pred[[1, 1]], NO_PRED_NODE);
        assert_eq!(pred[[1, 2]], 0);
        assert_eq!(pred[[1, 3]], 1);

        assert_eq!(pred[[2, 0]], 2);
        assert_eq!(pred[[2, 1]], 0);
        assert_eq!(pred[[2, 2]], NO_PRED_NODE);
        assert_eq!(pred[[2, 3]], 2);

        assert_eq!(pred[[3, 0]], 1);
        assert_eq!(pred[[3, 1]], 3);
        assert_eq!(pred[[3, 2]], 3);
        assert_eq!(pred[[3, 3]], NO_PRED_NODE);
    }

    #[test]
    fn test_floyd_warshall_scipy() {
        let mut dist = setup_scipy_test();
        let pred = shortest_path(&mut dist, Algorithm::Floyd);
        compare_scipy_test(dist, pred);
    }

    #[test]
    fn test_dijkstra_scipy() {
        let mut dist = setup_scipy_test();
        let pred = shortest_path(&mut dist, Algorithm::Dijkstra);
        compare_scipy_test(dist, pred);
    }

    #[test]
    fn test_dial_scipy() {
        let mut dist = setup_scipy_test();
        let pred = shortest_path(&mut dist, Algorithm::Dial);
        compare_scipy_test(dist, pred);
    }

    fn setup_random_matrix(vertexes: usize, edges: usize) -> Array2<u16> {
        let mut rng = thread_rng();
        let max_cost = 4;
        let mut dist = Array2::<u16>::from_elem((vertexes, vertexes), INFINITY);
        for _ in 0..edges {
            let i = rng.gen_range(0..vertexes);
            let j = rng.gen_range(0..vertexes);
            let cost = rng.gen_range(1..=max_cost);
            dist[[i, j]] = cost as u16;
        }
        dist
    }

    #[test]
    fn test_multi_algorithm_random_matrix() {
        let mut dist1 = setup_random_matrix(100, 1000);
        let mut dist2 = dist1.clone();
        let mut dist3 = dist2.clone();

        floyd_warshall(&mut dist1);
        dijkstra(&mut dist2);
        dial(&mut dist3);

        assert_eq!(dist1, dist2);
        assert_eq!(dist1, dist3);
        // predecessors are not guaranteed to be identical
    }

    #[test]
    fn test_two_algorithms_bigger_random_matrix() {
        let mut dist1 = setup_random_matrix(1000, 6000);
        let mut dist2 = dist1.clone();

        dijkstra(&mut dist1);
        dial(&mut dist2);

        assert_eq!(dist1, dist2);
        // predecessors are not guaranteed to be identical
    }

    #[test]
    #[should_panic(expected = "invalid Algorithm")]
    fn test_dijkstra_dial_inner_bad_algorithm() {
        let mut dist = setup_random_matrix(100, 1000);
        dijkstra_dial_inner(&mut dist, Algorithm::Floyd);
    }
}
