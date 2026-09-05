use std::collections::{HashMap, HashSet};

use rand::seq::SliceRandom;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{
        Rating, SaveTierlistRequest, TierlistEntry, TierlistGatekeeper, TierlistResponse,
        TierlistTier,
    },
    repositories,
    state::AppState,
};

const TIER_ORDER: [&str; 6] = ["S", "A", "B", "C", "D", "F"];

pub async fn get_tierlist(state: &AppState, user_id: Uuid) -> AppResult<TierlistResponse> {
    let has_entries = repositories::has_tierlist_entries(&state.pool, user_id).await?;

    if !has_entries {
        let rated = repositories::list_rated_series(&state.pool, user_id).await?;
        if !rated.is_empty() {
            let mut by_tier: HashMap<String, Vec<Uuid>> = HashMap::new();
            for (series_id, rating) in rated {
                if let Some(tier) = rating.tier_value() {
                    by_tier.entry(tier.to_string()).or_default().push(series_id);
                }
            }

            let mut seed_entries = Vec::new();
            {
                let mut rng = rand::thread_rng();
                for tier in TIER_ORDER {
                    if let Some(mut ids) = by_tier.remove(tier) {
                        ids.shuffle(&mut rng);
                        for (position, series_id) in ids.iter().enumerate() {
                            seed_entries.push((*series_id, tier.to_string(), position as i64));
                        }
                    }
                }
            }

            repositories::insert_tierlist_entries(&state.pool, user_id, &seed_entries).await?;
        }
    }

    build_response(state, user_id).await
}

pub async fn save_tierlist(
    state: &AppState,
    user_id: Uuid,
    body: SaveTierlistRequest,
) -> AppResult<TierlistResponse> {
    let mut tiers = Vec::new();
    let mut membership: HashMap<String, HashSet<Uuid>> = HashMap::new();

    for tier_input in body.tiers {
        if Rating::from_tier_str(&tier_input.tier).is_none() {
            return Err(AppError::BadRequest(format!(
                "invalid tier: {}",
                tier_input.tier
            )));
        }
        membership.insert(
            tier_input.tier.clone(),
            tier_input.series_ids.iter().copied().collect(),
        );
        tiers.push((tier_input.tier, tier_input.series_ids));
    }

    let gate_rows = validate_gatekeepers(&body.gatekeepers, &membership)?;

    repositories::replace_tierlist(&state.pool, user_id, &tiers).await?;
    repositories::replace_tierlist_gatekeepers(&state.pool, user_id, &gate_rows).await?;
    build_response(state, user_id).await
}

fn validate_gatekeepers(
    gatekeepers: &[TierlistGatekeeper],
    membership: &HashMap<String, HashSet<Uuid>>,
) -> AppResult<Vec<(String, String, Uuid)>> {
    let mut rows = Vec::new();
    let mut seen_slots = HashSet::new();

    for g in gatekeepers {
        if Rating::from_tier_str(&g.tier).is_none() {
            return Err(AppError::BadRequest(format!("invalid gatekeeper tier: {}", g.tier)));
        }
        let members = membership.get(&g.tier).cloned().unwrap_or_default();

        if let Some(id) = g.above_series_id {
            if g.tier == "S" {
                return Err(AppError::BadRequest(
                    "S 티어에는 윗 수문장을 둘 수 없습니다".into(),
                ));
            }
            if !members.contains(&id) {
                return Err(AppError::BadRequest(format!(
                    "{} 티어 윗 수문장은 같은 티어 작품이어야 합니다",
                    g.tier
                )));
            }
            let key = format!("{}:above", g.tier);
            if !seen_slots.insert(key) {
                return Err(AppError::BadRequest(format!(
                    "duplicate above gatekeeper for tier {}",
                    g.tier
                )));
            }
            rows.push((g.tier.clone(), "above".to_string(), id));
        }

        if let Some(id) = g.below_series_id {
            if g.tier == "F" {
                return Err(AppError::BadRequest(
                    "F 티어에는 아래 수문장을 둘 수 없습니다".into(),
                ));
            }
            if !members.contains(&id) {
                return Err(AppError::BadRequest(format!(
                    "{} 티어 아래 수문장은 같은 티어 작품이어야 합니다",
                    g.tier
                )));
            }
            if g.above_series_id == Some(id) {
                return Err(AppError::BadRequest(
                    "같은 작품을 윗·아래 수문장으로 동시에 지정할 수 없습니다".into(),
                ));
            }
            let key = format!("{}:below", g.tier);
            if !seen_slots.insert(key) {
                return Err(AppError::BadRequest(format!(
                    "duplicate below gatekeeper for tier {}",
                    g.tier
                )));
            }
            rows.push((g.tier.clone(), "below".to_string(), id));
        }
    }

    Ok(rows)
}

/// Read-only tierlist snapshot (no seed writes). Used by the first-login tour demo.
pub async fn get_tierlist_snapshot(
    state: &AppState,
    user_id: Uuid,
) -> AppResult<TierlistResponse> {
    build_response(state, user_id).await
}

async fn build_response(state: &AppState, user_id: Uuid) -> AppResult<TierlistResponse> {
    let rows = repositories::list_tierlist_entries(&state.pool, user_id).await?;
    let mut grouped: HashMap<String, Vec<TierlistEntry>> = HashMap::new();

    for (tier, series_id, title, cover, position) in rows {
        grouped.entry(tier).or_default().push(TierlistEntry {
            series_id,
            title,
            cover_url: cover,
            position,
        });
    }

    let tiers = TIER_ORDER
        .iter()
        .map(|tier| {
            let mut entries = grouped.remove(*tier).unwrap_or_default();
            entries.sort_by_key(|e| e.position);
            TierlistTier {
                tier: (*tier).to_string(),
                entries,
            }
        })
        .collect();

    let none_entries = repositories::list_none_tier_candidates(&state.pool, user_id)
        .await?
        .into_iter()
        .map(|(series_id, title, cover_url, position)| TierlistEntry {
            series_id,
            title,
            cover_url,
            position,
        })
        .collect();

    let mut gate_map: HashMap<String, TierlistGatekeeper> = TIER_ORDER
        .iter()
        .map(|t| {
            (
                (*t).to_string(),
                TierlistGatekeeper {
                    tier: (*t).to_string(),
                    above_series_id: None,
                    below_series_id: None,
                },
            )
        })
        .collect();

    for (tier, side, series_id) in repositories::list_tierlist_gatekeepers(&state.pool, user_id)
        .await?
    {
        if let Some(slot) = gate_map.get_mut(&tier) {
            match side.as_str() {
                "above" => slot.above_series_id = Some(series_id),
                "below" => slot.below_series_id = Some(series_id),
                _ => {}
            }
        }
    }

    let gatekeepers = TIER_ORDER
        .iter()
        .filter_map(|t| gate_map.remove(*t))
        .collect();

    Ok(TierlistResponse {
        tiers,
        none_entries,
        gatekeepers,
    })
}
