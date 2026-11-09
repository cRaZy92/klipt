ALTER TABLE "clips" ALTER COLUMN "created_at" SET DATA TYPE timestamp (3) with time zone;--> statement-breakpoint
ALTER TABLE "clips" ALTER COLUMN "created_at" SET DEFAULT now();