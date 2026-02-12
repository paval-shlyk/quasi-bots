use std::collections::HashMap;

use crate::knowledge::Topic;

#[derive(Debug)]
pub struct TopicSequence {
    topics: Vec<Topic>,
    ///index to start
    next_topic: usize,
}

impl TopicSequence {
    pub fn from_slice(topics: &[Topic]) -> Self {
        let set = HashMap::<String, Topic>::from_iter(
            topics.iter().map(|t| (t.name.clone(), t.clone())),
        );

        assert_eq!(set.len(), topics.len(), "Duplicated topics are detected");

        Self {
            topics: topics.to_vec(),
            next_topic: 0,
        }
    }

    pub fn next(&mut self) -> Option<Topic> {
        if self.next_topic >= self.topics.len() {
            return None;
        }

        let unused_topics = &self.topics[self.next_topic..];
        let idx = rand::random_range(0..unused_topics.len());

        let topic = unused_topics[idx].clone();

        self.topics.swap(self.next_topic, idx + self.next_topic);
        self.next_topic += 1;

        Some(topic)
    }

    pub fn try_push(&mut self, topic: Topic) -> anyhow::Result<()> {
        let is_duplicate = self.topics.iter().any(|old| old.name == topic.name);

        if is_duplicate {
            return Err(anyhow::anyhow!("Duplicated topic"));
        }

        self.topics.push(topic);
        Ok(())
    }

    // Reset topic counter
    pub fn reset(&mut self) {
        self.next_topic = 0;
    }

    /// Total count of topics
    pub fn len(&self) -> usize {
        self.topics.len()
    }

    pub fn to_vec(&self) -> Vec<Topic> {
        self.topics.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::HashSet;

    prop_compose! {
        fn arb_topic()(id in any::<u64>(), name in "[a-zA-Z0-9_]+") -> Topic {
            Topic { id, name }
        }
    }

    proptest! {
        #[test]
        fn test_sequence_exhaustion(
            raw_topics in proptest::collection::vec(arb_topic(), 1..100)
        ) {
            // Deduplicate inputs by name
            let mut topics = Vec::new();
            let mut seen_names = HashSet::new();
            for t in raw_topics {
                if seen_names.insert(t.name.clone()) {
                    topics.push(t);
                }
            }

            let mut seq = TopicSequence::from_slice(&topics);

            let mut seen_names_out = HashSet::new();
            for _ in 0..topics.len() {
                let t = seq.next();
                prop_assert!(t.is_some());
                let t = t.unwrap();
                prop_assert!(seen_names_out.insert(t.name.clone())); // Ensure uniqueness of output
                prop_assert!(topics.iter().any(|input| input.name == t.name)); // Ensure it belongs to input
            }

            prop_assert!(seq.next().is_none());
        }

        #[test]
        fn test_try_push_success(
            raw_topics in proptest::collection::vec(arb_topic(), 0..50),
            new_topic in arb_topic()
        ) {
            let mut topics = Vec::new();
            let mut seen_names = HashSet::new();

            // Ensure new_topic name is not in initial list
            seen_names.insert(new_topic.name.clone());

            for t in raw_topics {
                if seen_names.insert(t.name.clone()) {
                    topics.push(t);
                }
            }

            let mut seq = TopicSequence::from_slice(&topics);

            prop_assert!(seq.try_push(new_topic.clone()).is_ok());

            // Verify count through exhaustion
            let mut count = 0;
            let mut found = false;
            while let Some(t) = seq.next() {
                if t.name == new_topic.name { found = true; }
                count += 1;
            }
            prop_assert_eq!(count, topics.len() + 1);
            prop_assert!(found);
        }

        #[test]
        fn test_try_push_duplicate(
             raw_topics in proptest::collection::vec(arb_topic(), 1..50)
        ) {
             let mut topics = Vec::new();
             let mut seen_names = HashSet::new();
             for t in raw_topics {
                 if seen_names.insert(t.name.clone()) {
                     topics.push(t);
                 }
             }

             if topics.is_empty() { return Ok(()); }

             let mut seq = TopicSequence::from_slice(&topics);

             let duplicate = topics[0].clone();
             prop_assert!(seq.try_push(duplicate).is_err());
        }

        #[test]
        fn test_reset(
            raw_topics in proptest::collection::vec(arb_topic(), 1..50)
        ) {
            let mut topics = Vec::new();
            let mut seen_names = HashSet::new();
            for t in raw_topics {
                if seen_names.insert(t.name.clone()) {
                    topics.push(t);
                }
            }

            let mut seq = TopicSequence::from_slice(&topics);

            // Consume partial
            if !topics.is_empty() {
                let _ = seq.next();
            }

            seq.reset();

            // Should be full length again
            let mut count = 0;
            while seq.next().is_some() {
                count += 1;
            }
            prop_assert_eq!(count, topics.len());
        }
    }
}
