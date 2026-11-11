CREATE TABLE "clip_tags" (
	"clip_id" text NOT NULL,
	"user_id" uuid NOT NULL,
	CONSTRAINT "clip_tags_clip_id_user_id_pk" PRIMARY KEY("clip_id","user_id")
);
--> statement-breakpoint
ALTER TABLE "clip_tags" ADD CONSTRAINT "clip_tags_clip_id_clips_id_fk" FOREIGN KEY ("clip_id") REFERENCES "public"."clips"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "clip_tags" ADD CONSTRAINT "clip_tags_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "clip_tags_user_idx" ON "clip_tags" USING btree ("user_id");