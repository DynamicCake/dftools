use redis::AsyncCommands;
use redis_macros::{FromRedisValue, ToRedisArgs};
use serde::{Deserialize, Serialize};
use sqlx::{query, query_as};

use crate::api::PlotId;

use super::Store;

#[derive(Serialize, Deserialize, ToRedisArgs, FromRedisValue)]
pub struct TrustVec(Vec<PlotId>);

/// Baton
impl Store {
    pub async fn fetch_plot_trust(&self, plot: PlotId) -> color_eyre::Result<Vec<PlotId>> {
        let mut redis = self.redis.clone();
        let attempt: Option<TrustVec> = redis.get(format!("plot:{}:baton_trust", plot)).await?;
        Ok(if let Some(trusts) = attempt {
            trusts.0
        } else {
            struct TrustRow {
                trusted: PlotId,
            }
            let trusts: Vec<PlotId> = query_as!(
                TrustRow,
                "SELECT trusted FROM baton_trust WHERE plot = $1;",
                plot
            )
            .fetch_all(&self.pg)
            .await?
            .into_iter()
            .map(|it| it.trusted)
            .collect();

            let trusts = TrustVec(trusts);

            let _: () = redis
                .set(format!("plot:{}:baton_trust", plot), &trusts)
                .await?;
            trusts.0
        })
    }
    pub async fn set_plot_trust(
        &self,
        plot_id: PlotId,
        trusts: Vec<PlotId>,
    ) -> color_eyre::Result<Result<(), PlotTrustSetError>> {
        let mut tx = self.pg.begin().await?;
        let affected = query!("SELECT id FROM plot WHERE id = $1", plot_id)
            .fetch_optional(&mut *tx)
            .await?;
        if affected.is_none() {
            return Ok(Err(PlotTrustSetError::PlotNotFound));
        }

        query!("DELETE FROM baton_trust WHERE id = $1", plot_id)
            .execute(&mut *tx)
            .await?;

        for trust in trusts {
            query!(
                "INSERT INTO baton_trust (plot, trusted) VALUES ($1, $2) 
                ON CONFLICT (plot, trusted) DO NOTHING",
                plot_id,
                trust
            )
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        self.invalidate_trust_cache(plot_id).await?;
        Ok(Ok(()))
    }

    async fn invalidate_trust_cache(&self, plot_id: PlotId) -> color_eyre::Result<()> {
        let _: () = self
            .redis
            .clone()
            .del(format!("plot:{}:baton_trust", plot_id))
            .await?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PlotTrustSetError {
    #[error("Plot not found")]
    PlotNotFound,
}
