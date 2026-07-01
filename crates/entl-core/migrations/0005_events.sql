-- Raw GitHub event stream (the /repos/{o}/{r}/events feed). Entl polls this as the
-- top-level "did anything happen?" signal AND stores every event as a queryable
-- activity log. Note: GitHub's feed is capped (~300 events / 90 days), so this is
-- complete *going forward* from when polling starts, not a full historical backfill.
-- `payload` is the event's type-specific JSON, stored as text (query via
-- json_extract(payload, '$.…') / payload::JSON).

CREATE TABLE "gh_events" (
	"repo_id" text NOT NULL,
	"id" text NOT NULL,
	"type" text,
	"actor_id" bigint,
	"actor_login" text,
	"created_at" TIMESTAMP,
	"payload" text,
	CONSTRAINT "events_pk" PRIMARY KEY ("repo_id", "id")
);
