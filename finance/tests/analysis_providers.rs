//! Provider contract checks using test doubles from `helpers`.

mod helpers;

use finance::analysis::{
    EarningsCalendarProvider, NewsProvider, PriceTargetProvider,
};
use helpers::{
    MockEarningsCalendarProvider, MockNewsProvider, MockPriceTargetProvider,
};

#[tokio::test]
async fn given_mock_providers_when_enrich_fields_then_targets_earnings_news_present()
 {
    // Arrange
    let targets = MockPriceTargetProvider {
        mean: 150.0,
        high: 180.0,
        low: 120.0,
    };
    let earnings = MockEarningsCalendarProvider;
    let news = MockNewsProvider { limit: 3 };

    // Act
    let t = targets.targets("TEST").await.unwrap();
    let e = earnings.earnings("TEST").await.unwrap();
    let n = news.recent("TEST", Some("Test")).await.unwrap();

    // Assert
    assert_eq!(t.mean, Some(150.0));
    assert_eq!(t.source, "mock");
    assert!(e.next_report_at.is_some());
    assert_eq!(e.source, "mock");
    assert_eq!(n.len(), 1);
    assert!(n[0].title.contains("TEST"));
}
