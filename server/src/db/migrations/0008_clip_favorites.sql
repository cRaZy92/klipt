CREATE TABLE "clip_favorites" (
	"clip_id" text NOT NULL,
	"user_id" uuid NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "clip_favorites_clip_id_user_id_pk" PRIMARY KEY("clip_id","user_id")
);
--> statement-breakpoint
ALTER TABLE "clip_favorites" ADD CONSTRAINT "clip_favorites_clip_id_clips_id_fk" FOREIGN KEY ("clip_id") REFERENCES "public"."clips"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "clip_favorites" ADD CONSTRAINT "clip_favorites_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "clip_favorites_user_idx" ON "clip_favorites" USING btree ("user_id");