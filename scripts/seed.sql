-- Nova Reader Seed Data
-- Usage: psql -U nova -d nova_reader < scripts/seed.sql

-- Insert sample library
INSERT INTO libraries (id, name, path, scan_interval_secs)
VALUES ('00000000-0000-0000-0000-000000000001', '测试书库', '/data/library', 3600)
ON CONFLICT (id) DO NOTHING;

-- Insert sample books
INSERT INTO books (id, library_id, title, author, description, language, format, status, file_path, file_hash, file_size_bytes, word_count, chapter_count, reading_status)
VALUES
  ('00000000-0000-0000-0000-000000000010', '00000000-0000-0000-0000-000000000001',
   '斗破苍穹', '天蚕土豆', '萧炎，主人公，天才少年，曾经的天之骄子，三年前却成为了废物...',
   'chinese', 'txt', 'completed', '/data/library/dpcq.txt', 'hash001', 5242880, 3500000, 1500, 'reading'),
  ('00000000-0000-0000-0000-000000000011', '00000000-0000-0000-0000-000000000001',
   '凡人修仙传', '忘语', '一个普通的山村穷小子，偶然之下进入到当地的江湖门派...',
   'chinese', 'txt', 'completed', '/data/library/frxxt.txt', 'hash002', 8388608, 7450000, 2446, 'unread'),
  ('00000000-0000-0000-0000-000000000012', '00000000-0000-0000-0000-000000000001',
   '诡秘之主', '爱潜水的乌贼', '蒸汽与机械的浪潮中，谁能触及非凡？在神秘复苏的时代...',
   'chinese', 'txt', 'completed', '/data/library/gmzz.txt', 'hash003', 6291456, 5000000, 1394, 'reading')
ON CONFLICT (id) DO NOTHING;

-- Insert sample chapters for the first book
INSERT INTO chapters (id, book_id, chapter_index, title, word_count, content)
VALUES
  (gen_random_uuid(), '00000000-0000-0000-0000-000000000010', 0, '第一章 陨落的天才', 2800,
   '斗气大陆，无尽无际。在这片大陆上，没有花俏艳丽的魔法，有的，仅仅是繁衍到巅峰的斗气！斗气，决定一切，在这里，谁的拳头大，谁就是老大！'),
  (gen_random_uuid(), '00000000-0000-0000-0000-000000000010', 1, '第二章 斗气大陆', 3200,
   '乌坦城，一座建立在魔兽山脉旁的城市。由于靠近山脉，城中猎人公会等等级颇高。而且这座城市也是萧家的据点。'),
  (gen_random_uuid(), '00000000-0000-0000-0000-000000000010', 2, '第三章 客卿：药老', 2600,
   '在萧炎的心中，药老是一位极为神秘的存在。按照老者的说辞，他只不过是在萧炎体内寄居的一缕灵魂。')
ON CONFLICT DO NOTHING;

-- Insert sample chapters for the second book
INSERT INTO chapters (id, book_id, chapter_index, title, word_count, content)
VALUES
  (gen_random_uuid(), '00000000-0000-0000-0000-000000000011', 0, '第一章 先生', 3100,
   '太南山区，这个地方距离最近的都市大概有八百多里。它跟南方所有的山区一样，有着各种各样的野兽出没。'),
  (gen_random_uuid(), '00000000-0000-0000-0000-000000000011', 1, '第二章 墨大夫', 2900,
   '三天后的傍晚。韩立把手中草药采集结束，从山上走了下来。')
ON CONFLICT DO NOTHING;

-- Insert sample reading progress
INSERT INTO reading_progress (id, book_id, progress, current_chapter, chapter_index, reading_time_secs, last_read_at)
VALUES
  (gen_random_uuid(), '00000000-0000-0000-0000-000000000010', 0.35, 2, 2, 7200, NOW() - INTERVAL '2 hours')
ON CONFLICT (book_id) DO NOTHING;

-- Insert sample reading sessions (for streak/heatmap)
INSERT INTO reading_sessions (id, book_id, started_at, ended_at, duration_secs, pages_read, words_read)
VALUES
  (gen_random_uuid(), '00000000-0000-0000-0000-000000000010', NOW() - INTERVAL '1 day', NOW() - INTERVAL '1 day' + INTERVAL '30 minutes', 1800, 10, 3000),
  (gen_random_uuid(), '00000000-0000-0000-0000-000000000010', NOW() - INTERVAL '2 days', NOW() - INTERVAL '2 days' + INTERVAL '45 minutes', 2700, 15, 4500),
  (gen_random_uuid(), '00000000-0000-0000-0000-000000000010', NOW() - INTERVAL '3 days', NOW() - INTERVAL '3 days' + INTERVAL '20 minutes', 1200, 7, 2100),
  (gen_random_uuid(), '00000000-0000-0000-0000-000000000012', NOW(), NOW() + INTERVAL '15 minutes', 900, 5, 1500)
ON CONFLICT DO NOTHING;

-- Insert sample entities
INSERT INTO entities (id, name, entity_type, description, book_id)
VALUES
  (gen_random_uuid(), '萧炎', 'character', '主角，萧家少年天才，后遭遇三年废物期', '00000000-0000-0000-0000-000000000010'),
  (gen_random_uuid(), '药老', 'character', '药尘，斗帝强者，以灵魂形态寄居在萧炎戒指中', '00000000-0000-0000-0000-000000000010'),
  (gen_random_uuid(), '乌坦城', 'location', '萧家据点所在城市', '00000000-0000-0000-0000-000000000010'),
  (gen_random_uuid(), '韩立', 'character', '主角，凡人修仙传', '00000000-0000-0000-0000-000000000011'),
  (gen_random_uuid(), '克莱恩', 'character', '主角，诡秘之主', '00000000-0000-0000-0000-000000000012')
ON CONFLICT DO NOTHING;

-- Insert sample annotations
INSERT INTO annotations (id, book_id, chapter_index, start_offset, end_offset, selected_text, note, color, chapter_id)
SELECT gen_random_uuid(), '00000000-0000-0000-0000-000000000010', 0, 0, 12, '斗气大陆', '这是整个世界的设定', 'yellow',
       (SELECT id FROM chapters WHERE book_id = '00000000-0000-0000-0000-000000000010' AND chapter_index = 0 LIMIT 1)
WHERE EXISTS (SELECT 1 FROM chapters WHERE book_id = '00000000-0000-0000-0000-000000000010');

-- Insert sample reading goals
INSERT INTO reading_goals (id, goal_type, target_value, current_value, period_start, period_end)
VALUES
  (gen_random_uuid(), 'daily_minutes', 30, 15, CURRENT_DATE, CURRENT_DATE + INTERVAL '1 day'),
  (gen_random_uuid(), 'weekly_books', 2, 1, DATE_TRUNC('week', CURRENT_DATE), DATE_TRUNC('week', CURRENT_DATE) + INTERVAL '7 days')
ON CONFLICT DO NOTHING;

-- Done
SELECT 'Seed data inserted: 3 books, 5 chapters, 4 sessions, 5 entities, 1 annotation, 2 goals' as status;
