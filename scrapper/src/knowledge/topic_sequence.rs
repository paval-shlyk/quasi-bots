use std::collections::HashSet;

use crate::knowledge::Topic;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TopicSequence {
    topics: Vec<Topic>,
    ///index to start
    next_topic: usize,
}

impl TopicSequence {
    pub fn from_slice(topics: &[Topic]) -> Self {
        let set = HashSet::<Topic>::from_iter(topics.iter().cloned());

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
        let is_duplicate =
            self.topics.iter().find(|old| **old == topic).is_some();

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

    proptest! {
        #[test]
        fn test_sequence_exhaustion(
            topics in proptest::collection::hash_set(".*", 1..100)
        ) {
            let topic_list: Vec<Topic> = topics.iter().cloned().collect();
            let mut seq = TopicSequence::from_slice(&topic_list);

            let mut seen = HashSet::new();
            for _ in 0..topics.len() {
                let t = seq.next();
                prop_assert!(t.is_some());
                let t = t.unwrap();
                prop_assert!(seen.insert(t.clone())); // Ensure uniqueness of output
                prop_assert!(topics.contains(&t)); // Ensure it belongs to input
            }

            prop_assert!(seq.next().is_none());
        }

        #[test]
        fn test_try_push_success(
            mut topics in proptest::collection::hash_set(".*", 0..50),
            new_topic in ".*"
        ) {
            // Ensure new_topic is not in initial set
            if topics.contains(&new_topic) {
                topics.remove(&new_topic);
            }
            let topic_list: Vec<Topic> = topics.iter().cloned().collect();
            let mut seq = TopicSequence::from_slice(&topic_list);

            prop_assert!(seq.try_push(new_topic.clone()).is_ok());

            // Verify count through exhaustion
            let mut count = 0;
            let mut found = false;
            while let Some(t) = seq.next() {
                if t == new_topic { found = true; }
                count += 1;
            }
            prop_assert_eq!(count, topic_list.len() + 1);
            prop_assert!(found);
        }

        #[test]
        fn test_try_push_duplicate(
             topics in proptest::collection::hash_set(".*", 1..50)
        ) {
             let topic_list: Vec<Topic> = topics.iter().cloned().collect();
             let mut seq = TopicSequence::from_slice(&topic_list);

             let duplicate = topic_list[0].clone();
             prop_assert!(seq.try_push(duplicate).is_err());
        }

        #[test]
        fn test_reset(
            topics in proptest::collection::hash_set(".*", 1..50)
        ) {
            let topic_list: Vec<Topic> = topics.iter().cloned().collect();
            let mut seq = TopicSequence::from_slice(&topic_list);

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
