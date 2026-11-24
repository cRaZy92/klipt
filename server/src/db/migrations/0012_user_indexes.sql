CREATE INDEX "clips_user_idx" ON "clips" USING btree ("user_id","created_at" DESC NULLS LAST);--> statement-breakpoint
CREATE INDEX "sessions_user_idx" ON "sessions" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "uploads_user_idx" ON "uploads" USING btree ("user_id");