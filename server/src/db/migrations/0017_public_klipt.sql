CREATE TABLE "friendships" (
	"user_a" uuid NOT NULL,
	"user_b" uuid NOT NULL,
	"requested_by" uuid NOT NULL,
	"status" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"accepted_at" timestamp with time zone,
	CONSTRAINT "friendships_user_a_user_b_pk" PRIMARY KEY("user_a","user_b")
);
--> statement-breakpoint
ALTER TABLE "clips" ALTER COLUMN "visibility" SET DEFAULT 'friends';--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "profile_visibility" text DEFAULT 'friends' NOT NULL;--> statement-breakpoint
ALTER TABLE "friendships" ADD CONSTRAINT "friendships_user_a_users_id_fk" FOREIGN KEY ("user_a") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "friendships" ADD CONSTRAINT "friendships_user_b_users_id_fk" FOREIGN KEY ("user_b") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "friendships" ADD CONSTRAINT "friendships_requested_by_users_id_fk" FOREIGN KEY ("requested_by") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "friendships_user_b_idx" ON "friendships" USING btree ("user_b");--> statement-breakpoint
-- klipt:feed-to-friends
UPDATE "clips" SET "visibility" = 'friends' WHERE "visibility" = 'feed';--> statement-breakpoint
-- klipt:backfill-friends
INSERT INTO "friendships" ("user_a", "user_b", "requested_by", "status", "accepted_at")
SELECT a.id, b.id, a.id, 'accepted', now()
FROM "users" a JOIN "users" b ON a.id < b.id
WHERE a.disabled_at IS NULL AND b.disabled_at IS NULL
ON CONFLICT DO NOTHING;
