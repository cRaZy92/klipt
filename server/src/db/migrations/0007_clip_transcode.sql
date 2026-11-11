ALTER TABLE "clips" ADD COLUMN "transcode_state" text;--> statement-breakpoint
ALTER TABLE "clips" ADD COLUMN "media_version" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "clips" ADD COLUMN "original_bytes" bigint;--> statement-breakpoint
ALTER TABLE "clips" ADD COLUMN "transcode_error" text;--> statement-breakpoint
ALTER TABLE "clips" ADD COLUMN "transcode_started_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "clips" ADD COLUMN "transcode_finished_at" timestamp with time zone;--> statement-breakpoint
CREATE INDEX "clips_transcode_idx" ON "clips" USING btree ("transcode_state");--> statement-breakpoint
-- Clips uploaded before compression existed join the queue, so the server shrinks them too.
UPDATE "clips" SET "transcode_state" = 'queued' WHERE "kind" = 'clip';
