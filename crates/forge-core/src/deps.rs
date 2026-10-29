//! Dependencies between addons: cycle checks for the platform, install order for the launcher.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// The path that a new edge `from -> to` would close into a cycle, starting and ending at `from`.
pub fn find_cycle<T: Eq + Hash + Clone>(edges: &HashMap<T, Vec<T>>, from: &T, to: &T) -> Option<Vec<T>> {
    fn walk<T: Eq + Hash + Clone>(edges: &HashMap<T, Vec<T>>, at: &T, goal: &T, seen: &mut HashSet<T>, path: &mut Vec<T>) -> bool {
        path.push(at.clone());
        if at == goal {
            return true;
        }
        if seen.insert(at.clone()) {
            for next in edges.get(at).into_iter().flatten() {
                if walk(edges, next, goal, seen, path) {
                    return true;
                }
            }
        }
        path.pop();
        false
    }
    let mut path = vec![from.clone()];
    walk(edges, to, from, &mut HashSet::new(), &mut path).then_some(path)
}

/// Everything `root` needs, dependencies before what needs them, each once, `root` itself excluded.
pub fn install_order<T: Eq + Hash + Clone>(root: &T, edges: &HashMap<T, Vec<T>>) -> Vec<T> {
    fn visit<T: Eq + Hash + Clone>(at: &T, edges: &HashMap<T, Vec<T>>, seen: &mut HashSet<T>, order: &mut Vec<T>) {
        if !seen.insert(at.clone()) {
            return;
        }
        for next in edges.get(at).into_iter().flatten() {
            visit(next, edges, seen, order);
        }
        order.push(at.clone());
    }
    let mut order = Vec::new();
    visit(root, edges, &mut HashSet::new(), &mut order);
    order.retain(|t| t != root);
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(pairs: &[(&'static str, &'static str)]) -> HashMap<&'static str, Vec<&'static str>> {
        let mut edges: HashMap<_, Vec<_>> = HashMap::new();
        for (a, b) in pairs {
            edges.entry(*a).or_default().push(*b);
        }
        edges
    }

    #[test]
    fn direct_and_indirect_cycles() {
        let edges = graph(&[("a", "b"), ("b", "c")]);
        assert_eq!(find_cycle(&edges, &"c", &"a"), Some(vec!["c", "a", "b", "c"]));
        assert_eq!(find_cycle(&edges, &"b", &"a"), Some(vec!["b", "a", "b"]));
        assert_eq!(find_cycle(&edges, &"a", &"c"), None, "a shortcut is no cycle");
        assert_eq!(find_cycle(&edges, &"d", &"a"), None);
        assert_eq!(find_cycle(&HashMap::new(), &"a", &"a"), Some(vec!["a", "a"]), "self");
    }

    #[test]
    fn install_order_dependencies_first() {
        let edges = graph(&[("ui", "lib"), ("ui", "media"), ("media", "lib"), ("lib", "core")]);
        assert_eq!(install_order(&"ui", &edges), ["core", "lib", "media"]);
        assert!(install_order(&"core", &edges).is_empty());
        let looped = graph(&[("a", "b"), ("b", "a")]);
        assert_eq!(install_order(&"a", &looped), ["b"], "terminates on a cycle");
    }
}
