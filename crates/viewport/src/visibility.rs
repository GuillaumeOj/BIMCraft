//! What is hidden and what is isolated (VP-03, spec D6). Three inputs hide elements: categories,
//! disciplines and a set of elements ("hide selection"). An isolation then shows only what it
//! names, among what is not hidden. Isolating something shows it: it leaves the hidden sets.

use std::collections::BTreeSet;

use crate::ElementId;

/// What a command acts on, with the selection already resolved to elements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Categories(Vec<String>),
    Disciplines(Vec<String>),
    Elements(Vec<ElementId>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Isolation {
    Categories(BTreeSet<String>),
    Disciplines(BTreeSet<String>),
    Elements(BTreeSet<ElementId>),
}

/// The visibility state of the view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Visibility {
    hidden_categories: BTreeSet<String>,
    hidden_disciplines: BTreeSet<String>,
    hidden_elements: BTreeSet<ElementId>,
    isolation: Option<Isolation>,
}

fn set<T: Ord + Clone>(items: &[T]) -> BTreeSet<T> {
    items.iter().cloned().collect()
}

impl Visibility {
    /// Whether nothing is hidden or isolated.
    pub(crate) fn is_clear(&self) -> bool {
        *self == Self::default()
    }

    /// Whether the element is shown: not hidden, and inside the isolation if there is one.
    pub(crate) fn visible(&self, id: ElementId, category: &str, discipline: &str) -> bool {
        if self.hidden_categories.contains(category)
            || self.hidden_disciplines.contains(discipline)
            || self.hidden_elements.contains(&id)
        {
            return false;
        }
        match &self.isolation {
            None => true,
            Some(Isolation::Categories(names)) => names.contains(category),
            Some(Isolation::Disciplines(names)) => names.contains(discipline),
            Some(Isolation::Elements(ids)) => ids.contains(&id),
        }
    }

    pub(crate) fn hide(&mut self, target: &Target) {
        match target {
            Target::Categories(names) => self.hidden_categories.extend(names.iter().cloned()),
            Target::Disciplines(names) => self.hidden_disciplines.extend(names.iter().cloned()),
            Target::Elements(ids) => self.hidden_elements.extend(ids.iter().copied()),
        }
    }

    pub(crate) fn show(&mut self, target: &Target) {
        match target {
            Target::Categories(names) => names.iter().for_each(|n| {
                self.hidden_categories.remove(n);
            }),
            Target::Disciplines(names) => names.iter().for_each(|n| {
                self.hidden_disciplines.remove(n);
            }),
            Target::Elements(ids) => ids.iter().for_each(|id| {
                self.hidden_elements.remove(id);
            }),
        }
    }

    /// Shows only `target`, whatever hid it before.
    pub(crate) fn isolate(&mut self, target: &Target) {
        self.show(target);
        self.isolation = Some(match target {
            Target::Categories(names) => Isolation::Categories(set(names)),
            Target::Disciplines(names) => Isolation::Disciplines(set(names)),
            Target::Elements(ids) => Isolation::Elements(set(ids)),
        });
    }

    /// Ends the isolation; whether there was one.
    pub(crate) fn end_isolation(&mut self) -> bool {
        self.isolation.take().is_some()
    }

    pub(crate) fn is_isolating(&self) -> bool {
        self.isolation.is_some()
    }

    /// Un-hides every element hidden by "hide selection".
    pub(crate) fn show_hidden_elements(&mut self) {
        self.hidden_elements.clear();
    }

    /// How many elements "hide selection" hid.
    pub(crate) fn hidden_element_count(&self) -> usize {
        self.hidden_elements.len()
    }

    pub(crate) fn category_hidden(&self, name: &str) -> bool {
        self.hidden_categories.contains(name)
    }

    pub(crate) fn discipline_hidden(&self, name: &str) -> bool {
        self.hidden_disciplines.contains(name)
    }

    pub(crate) fn category_isolated(&self, name: &str) -> bool {
        matches!(&self.isolation, Some(Isolation::Categories(n)) if n.contains(name))
    }

    pub(crate) fn discipline_isolated(&self, name: &str) -> bool {
        matches!(&self.isolation, Some(Isolation::Disciplines(n)) if n.contains(name))
    }

    /// Forgets elements the scene no longer holds. An isolation left with no element ends, since
    /// it would show an empty scene; returns whether it did.
    pub(crate) fn retain_elements(&mut self, held: impl Fn(ElementId) -> bool) -> bool {
        self.hidden_elements.retain(|&id| held(id));
        let Some(Isolation::Elements(ids)) = &mut self.isolation else {
            return false;
        };
        ids.retain(|&id| held(id));
        let ended = ids.is_empty();
        if ended {
            self.isolation = None;
        }
        ended
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> ElementId {
        ElementId([n; 16])
    }

    fn names(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn everything_is_visible_by_default() {
        let v = Visibility::default();
        assert!(v.is_clear() && v.visible(id(1), "Wall", "arch") && !v.is_isolating());
    }

    #[test]
    fn hidden_categories_disciplines_and_elements_each_hide() {
        let mut v = Visibility::default();
        v.hide(&Target::Categories(names(&["Wall"])));
        v.hide(&Target::Disciplines(names(&["mep"])));
        v.hide(&Target::Elements(vec![id(7)]));
        assert!(!v.visible(id(1), "Wall", "arch"));
        assert!(!v.visible(id(1), "Duct", "mep"));
        assert!(!v.visible(id(7), "Slab", "arch"));
        assert!(v.visible(id(1), "Slab", "arch"));
        assert!(v.category_hidden("Wall") && v.discipline_hidden("mep"));
        assert_eq!(v.hidden_element_count(), 1);
        v.show(&Target::Categories(names(&["Wall"])));
        v.show(&Target::Disciplines(names(&["mep"])));
        v.show_hidden_elements();
        assert!(v.is_clear());
    }

    #[test]
    fn isolation_keeps_only_what_it_names_among_what_is_not_hidden() {
        let mut v = Visibility::default();
        v.hide(&Target::Elements(vec![id(2)]));
        v.isolate(&Target::Categories(names(&["Wall"])));
        assert!(v.is_isolating() && v.category_isolated("Wall") && !v.category_isolated("Slab"));
        assert!(v.visible(id(1), "Wall", "arch"));
        assert!(!v.visible(id(1), "Slab", "arch"));
        // A hidden element stays hidden inside the isolated category.
        assert!(!v.visible(id(2), "Wall", "arch"));
        assert!(v.end_isolation() && !v.end_isolation());
        assert!(v.visible(id(1), "Slab", "arch"));
    }

    #[test]
    fn isolating_shows_what_was_hidden() {
        let mut v = Visibility::default();
        v.hide(&Target::Categories(names(&["Wall"])));
        v.isolate(&Target::Categories(names(&["Wall"])));
        assert!(v.visible(id(1), "Wall", "arch") && !v.category_hidden("Wall"));
        v.isolate(&Target::Disciplines(names(&["arch"])));
        assert!(v.discipline_isolated("arch") && !v.category_isolated("Wall"));
        assert!(v.visible(id(1), "Slab", "arch") && !v.visible(id(1), "Duct", "mep"));
        v.isolate(&Target::Elements(vec![id(3)]));
        assert!(v.visible(id(3), "Duct", "mep") && !v.visible(id(4), "Duct", "mep"));
    }

    #[test]
    fn elements_the_scene_lost_are_forgotten() {
        let mut v = Visibility::default();
        v.hide(&Target::Elements(vec![id(1), id(2)]));
        v.isolate(&Target::Elements(vec![id(3), id(4)]));
        assert!(!v.retain_elements(|e| e == id(2) || e == id(3)));
        assert_eq!(v.hidden_element_count(), 1);
        assert!(v.visible(id(3), "x", "y") && !v.visible(id(4), "x", "y"));
        assert!(v.retain_elements(|e| e == id(2)));
        assert!(!v.is_isolating() && v.visible(id(4), "x", "y"));
        assert!(!v.retain_elements(|_| false));
    }
}
