CREATE TABLE "clip_views" (
	"clip_id" text NOT NULL,
	"user_id" uuid NOT NULL,
	"viewed_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "clip_views_clip_id_user_id_pk" PRIMARY KEY("clip_id","user_id")
);
--> statement-breakpoint
ALTER TABLE "clips" ADD COLUMN "anon_views" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "clip_views" ADD CONSTRAINT "clip_views_clip_id_clips_id_fk" FOREIGN KEY ("clip_id") REFERENCES "public"."clips"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "clip_views" ADD CONSTRAINT "clip_views_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;