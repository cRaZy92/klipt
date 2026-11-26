CREATE TABLE "user_achievements" (
	"user_id" uuid NOT NULL,
	"achievement_id" text NOT NULL,
	"earned_at" timestamp with time zone DEFAULT now() NOT NULL,
	"seen_at" timestamp with time zone,
	"quiet" boolean DEFAULT false NOT NULL,
	CONSTRAINT "user_achievements_user_id_achievement_id_pk" PRIMARY KEY("user_id","achievement_id")
);
--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "avatar_version" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "looks" jsonb;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "activity" jsonb;--> statement-breakpoint
ALTER TABLE "user_achievements" ADD CONSTRAINT "user_achievements_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;