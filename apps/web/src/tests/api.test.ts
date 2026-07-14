import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock fetch globally before importing
const mockFetch = vi.fn();
vi.stubGlobal('fetch', mockFetch);

// Import after stubbing fetch
import { api } from '$lib/services/api';

describe('ApiClient', () => {
	beforeEach(() => {
		mockFetch.mockReset();
	});

	describe('getBooks', () => {
		it('should fetch books with default params', async () => {
			const mockResponse = {
				items: [{ id: '1', title: 'Test Book' }],
				total: 1,
				page: 1,
				per_page: 20
			};
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(mockResponse)
			});

			const result = await api.getBooks();
			expect(mockFetch).toHaveBeenCalledWith(
				'/api/books?',
				expect.objectContaining({
					headers: expect.objectContaining({ 'Content-Type': 'application/json' })
				})
			);
			expect(result).toEqual(mockResponse);
		});

		it('should pass query parameters', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({ items: [], total: 0 })
			});

			await api.getBooks({ page: 2, per_page: 10, status: 'ready' });
			const url = mockFetch.mock.calls[0][0];
			expect(url).toContain('page=2');
			expect(url).toContain('per_page=10');
			expect(url).toContain('status=ready');
		});

		it('should throw on HTTP error', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: false,
				status: 404,
				statusText: 'Not Found',
				json: () => Promise.resolve({ error: { code: 404, message: 'Book not found', retryable: false } })
			});

			await expect(api.getBooks()).rejects.toThrow(/Book not found|Not Found/);
		});
	});

	describe('getBook', () => {
		it('should fetch a single book by id', async () => {
			const mockBook = { id: 'abc-123', title: '斗破苍穹', status: 'ready' };
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(mockBook)
			});

			const result = await api.getBook('abc-123');
			expect(mockFetch).toHaveBeenCalledWith(
				'/api/books/abc-123',
				expect.anything()
			);
			expect(result).toEqual(mockBook);
		});

		it('should update reading status without sending processing status', async () => {
			const updatedBook = {
				id: 'abc-123',
				title: '斗破苍穹',
				status: 'ready',
				reading_status: 'completed',
			};
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(updatedBook)
			});

			const result = await api.updateBookReadingStatus('abc-123', 'completed');

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/books/abc-123',
				expect.objectContaining({
					method: 'PUT',
					body: JSON.stringify({ reading_status: 'completed' })
				})
			);
			expect(result).toEqual(updatedBook);
		});

		it('should update editable book metadata and rating', async () => {
			const updatedBook = {
				id: 'abc-123',
				title: '斗破苍穹',
				status: 'ready',
				reading_status: 'reading',
				rating: 4,
				language: 'zh',
				genres: ['玄幻'],
				tags: ['成长']
			};
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(updatedBook)
			});

			const payload = {
				title: '斗破苍穹',
				language: 'zh' as const,
				genres: ['玄幻'],
				tags: ['成长'],
				rating: 4
			};
			const result = await api.updateBook('abc-123', payload);

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/books/abc-123',
				expect.objectContaining({
					method: 'PUT',
					body: JSON.stringify(payload)
				})
			);
			expect(result.rating).toBe(4);
		});
	});

	describe('Libraries', () => {
		it('should create a library', async () => {
			const mockLib = { id: 'lib-1', name: 'Novels', root_path: '/data/novels' };
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(mockLib)
			});

			const result = await api.createLibrary({ name: 'Novels', root_path: '/data/novels' });
			expect(mockFetch).toHaveBeenCalledWith(
				'/api/libraries',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({ name: 'Novels', root_path: '/data/novels' })
				})
			);
			expect(result).toEqual(mockLib);
		});

		it('should fetch a single library by id', async () => {
			const mockLib = { id: 'lib-1', name: 'Novels', root_path: '/data/novels' };
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(mockLib)
			});

			const result = await api.getLibrary('lib-1');

			expect(mockFetch).toHaveBeenCalledWith('/api/libraries/lib-1', expect.anything());
			expect(result).toEqual(mockLib);
		});

		it('should trigger a library scan', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({ task_id: 'task-1' })
			});

			await api.scanLibrary('lib-1');
			expect(mockFetch).toHaveBeenCalledWith(
				'/api/libraries/lib-1/scan',
				expect.objectContaining({ method: 'POST' })
			);
		});

		it('should fetch library scan status through the canonical library API surface', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({
					status: 'complete',
					total_files: 12,
					processed_files: 12,
					new_books: 2,
					errors: [],
				})
			});

			const result = await api.getLibraryScanStatus('lib-1');

			expect(mockFetch).toHaveBeenCalledWith('/api/libraries/lib-1/scan-status', expect.anything());
			expect(result.status).toBe('complete');
		});


		it('should enqueue a library maintenance action', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({ task_id: 'task-1', action: 'reindex' })
			});

			const result = await api.runLibraryMaintenance('lib-1', 'reindex');

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/libraries/lib-1/maintenance/reindex',
				expect.objectContaining({ method: 'POST' })
			);
			expect(result.task_id).toBe('task-1');
		});

		it('should fetch user and group library permissions', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({
					permissions: [{ user_id: 'user-1', can_read: true, can_write: false, can_manage: false }],
					group_permissions: [{ group_id: 'group-1', can_read: true, can_write: true, can_manage: false }]
				})
			});

			const result = await api.getLibraryPermissions('lib-1');

			expect(mockFetch).toHaveBeenCalledWith('/api/libraries/lib-1/permissions', expect.anything());
			expect(result.permissions).toHaveLength(1);
			expect(result.group_permissions).toEqual([
				{ group_id: 'group-1', can_read: true, can_write: true, can_manage: false }
			]);
		});

		it('should set user and group library permissions together', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({ status: 'ok' })
			});

			await api.setLibraryPermissions('lib-1', {
				permissions: [{ user_id: 'user-1', can_read: true, can_write: false, can_manage: false }],
				group_permissions: [{ group_id: 'group-1', can_read: true, can_write: true, can_manage: false }]
			});

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/libraries/lib-1/permissions',
				expect.objectContaining({
					method: 'PUT',
					body: JSON.stringify({
						permissions: [{ user_id: 'user-1', can_read: true, can_write: false, can_manage: false }],
						group_permissions: [{ group_id: 'group-1', can_read: true, can_write: true, can_manage: false }]
					})
				})
			);
		});
	});

	describe('Chapters', () => {
		it('should fetch chapter content', async () => {
			const mockContent = { content: '<p>第一章...</p>', entities: [] };
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(mockContent)
			});

			const result = await api.getChapterContent('book-1', 0);
			expect(mockFetch).toHaveBeenCalledWith(
				'/api/books/book-1/chapters/0/content',
				expect.anything()
			);
			expect(result.content).toBe('<p>第一章...</p>');
		});

		it('should unwrap chapter entities from the backend data envelope', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({
					data: [
						{ start_offset: 4, end_offset: 8, entity_type: 'character', name: '萧炎', id: 'entity-1' }
					]
				})
			});

			const result = await api.getChapterEntities('book-1', 0);

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/books/book-1/chapters/0/entities',
				expect.anything()
			);
			expect(result).toEqual([
				{ start: 4, end: 8, type: 'character', name: '萧炎', id: 'entity-1' }
			]);
		});
	});

	describe('Entity timelines', () => {
		it('should unwrap entity timelines from the mounted backend shape', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({
					entity_id: 'entity-1',
					timeline: [
						{
							chapter_index: 2,
							chapter_title: '风起',
							context: '萧炎第一次出手。',
							position: 128
						}
					]
				})
			});

			const result = await api.getEntityTimeline('entity-1');

			expect(mockFetch).toHaveBeenCalledWith('/api/entities/entity-1/timeline', expect.anything());
			expect(result).toEqual([
				{
					chapter_index: 2,
					chapter_title: '风起',
					context: '萧炎第一次出手。',
					position: 128
				}
			]);
		});
	});

	describe('Persons', () => {
		it('should fetch person detail and person books from mounted routes', async () => {
			mockFetch
				.mockResolvedValueOnce({
					ok: true,
					json: () => Promise.resolve({
						id: 'person-1',
						name: '天蚕土豆',
						original_name: null,
						avatar_path: null,
						roles: ['author'],
						book_count: 2,
						total_word_count: 3200000,
						biography: null
					})
				})
				.mockResolvedValueOnce({
					ok: true,
					json: () => Promise.resolve([
						{ id: 'book-1', title: '斗破苍穹', cover_path: null, word_count: 3200000, role: 'author' }
					])
				});

			const person = await api.getPerson('person-1');
			const books = await api.getPersonBooks('person-1');

			expect(mockFetch).toHaveBeenNthCalledWith(1, '/api/persons/person-1', expect.anything());
			expect(mockFetch).toHaveBeenNthCalledWith(2, '/api/persons/person-1/books', expect.anything());
			expect(person.name).toBe('天蚕土豆');
			expect(books[0].role).toBe('author');
		});
	});

	describe('Bookmarks', () => {
		it('should create bookmarks with the mounted backend field names', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({
					id: 'bookmark-1',
					book_id: 'book-1',
					chapter_index: 2,
					position: 0.42,
					title: '第三章',
					created_at: '2026-06-18T00:00:00Z'
				})
			});

			const result = await api.createBookmark('book-1', {
				title: '第三章',
				chapter_index: 2,
				position: 0.42
			});

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/books/book-1/bookmarks',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({
						title: '第三章',
						chapter_index: 2,
						position: 0.42
					})
				})
			);
			expect(result.title).toBe('第三章');
		});

		it('should delete bookmarks through the mounted backend route', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({})
			});

			await api.deleteBookmark('book-1', 'bookmark-1');

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/books/book-1/bookmarks/bookmark-1',
				expect.objectContaining({ method: 'DELETE' })
			);
		});
	});

	describe('Vibe bookmarks', () => {
		it('should save vibe bookmarks with the mounted backend field names', async () => {
			const mockBookmark = {
				id: 'vibe-1',
				name: '雪夜修炼',
				source_text: '雪夜中独自修炼',
				source_book_id: 'book-1',
				source_chapter_index: 3,
				created_at: '2026-06-18T00:00:00Z',
			};
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(mockBookmark)
			});

			const result = await api.saveVibeBookmark({
				name: '雪夜修炼',
				source_text: '雪夜中独自修炼',
				source_book_id: 'book-1',
				source_chapter_index: 3,
			});

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/search/vibe/bookmark',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({
						name: '雪夜修炼',
						source_text: '雪夜中独自修炼',
						source_book_id: 'book-1',
						source_chapter_index: 3,
					})
				})
			);
			expect(result.source_text).toBe('雪夜中独自修炼');
		});

		it('should list vibe bookmarks from the mounted backend route', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({
					data: [{
						id: 'vibe-1',
						name: '雪夜修炼',
						source_text: '雪夜中独自修炼',
						source_book_id: null,
						source_chapter_index: null,
						created_at: '2026-06-18T00:00:00Z',
					}]
				})
			});

			const result = await api.getVibeBookmarks();

			expect(mockFetch).toHaveBeenCalledWith('/api/search/vibe/bookmarks', expect.anything());
			expect(result).toEqual([{
				id: 'vibe-1',
				name: '雪夜修炼',
				source_text: '雪夜中独自修炼',
				source_book_id: null,
				source_chapter_index: null,
				created_at: '2026-06-18T00:00:00Z',
			}]);
		});
	});

	describe('Glossary creation', () => {
		it('should create glossary entries with the mounted backend field names', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({ id: 'glossary-1', term: '萧炎' })
			});

			const result = await api.createGlossaryEntry({
				term: '萧炎',
				definition: 'Xiao Yan',
				source_language: 'zh',
				target_language: 'en',
				book_id: 'book-1',
			});

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/glossary',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({
						term: '萧炎',
						definition: 'Xiao Yan',
						source_language: 'zh',
						target_language: 'en',
						book_id: 'book-1',
					})
				})
			);
			expect(result.term).toBe('萧炎');
		});
	});

	describe('Translation', () => {
		it('should keep the structured glossary match contract from the backend', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({
					translated_text: 'Xiao Yan entered the Dou Qi Continent.',
					glossary_applied: [{
						source_term: '斗气大陆',
						target_term: 'Dou Qi Continent',
						category: 'location',
					}],
					confidence: 0.95,
				})
			});

			const result = await api.translate({
				text: '萧炎来到斗气大陆。',
				source_language: 'zh',
				target_language: 'en',
				book_id: 'book-1',
				use_glossary: true,
			});

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/translate',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({
						text: '萧炎来到斗气大陆。',
						source_language: 'zh',
						target_language: 'en',
						book_id: 'book-1',
						use_glossary: true,
					})
				})
			);
			expect(result.glossary_applied[0]).toEqual({
				source_term: '斗气大陆',
				target_term: 'Dou Qi Continent',
				category: 'location',
			});
			expect(result.confidence).toBe(0.95);
		});
	});

	describe('Book custom fields', () => {
		it('should fetch and update book custom fields through mounted routes', async () => {
			mockFetch
				.mockResolvedValueOnce({
					ok: true,
					json: () => Promise.resolve({ source: 'Royal Road', edition: 2 })
				})
				.mockResolvedValueOnce({
					ok: true,
					json: () => Promise.resolve({ source: 'Royal Road', edition: 3 })
				});

			const fields = await api.getBookCustomFields('book-1');
			const updated = await api.updateBookCustomFields('book-1', { edition: 3 });

			expect(mockFetch).toHaveBeenNthCalledWith(1, '/api/books/book-1/custom-fields', expect.anything());
			expect(mockFetch).toHaveBeenNthCalledWith(
				2,
				'/api/books/book-1/custom-fields',
				expect.objectContaining({
					method: 'PUT',
					body: JSON.stringify({ edition: 3 })
				})
			);
			expect(fields.source).toBe('Royal Road');
			expect(updated.edition).toBe(3);
		});
	});

	describe('Search', () => {
		it('should search with query', async () => {
			const mockResults = { results: [{ id: '1', title: '凡人修仙传', score: 0.95 }], total: 1 };
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(mockResults)
			});

			const result = await api.search({ query: '修仙', page: 1 });
			expect(mockFetch).toHaveBeenCalledWith(
				'/api/search',
				expect.objectContaining({
					method: 'POST',
					body: expect.stringContaining('修仙')
				})
			);
			expect(result.results).toHaveLength(1);
		});

		it('should search across books with grouped mounted route payload', async () => {
			const mockResults = {
				groups: [
					{
						book_id: 'book-1',
						book_title: '凡人修仙传',
						count: 1,
						top_score: 0.92,
						chunks: [{ chunk_id: 'chunk-1', chapter_index: 3, chunk_index: 4, content: '筑基丹争夺', score: 0.92 }]
					}
				],
				total: 1,
				timing: { total_ms: 18 }
			};
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(mockResults)
			});

			const result = await api.searchCrossBook('筑基丹', ['book-1'], 12);

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/search/cross-book',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({
						query: '筑基丹',
						book_ids: ['book-1'],
						limit: 12,
						group_by_book: true
					})
				})
			);
			expect(result.groups[0].chunks[0].chunk_id).toBe('chunk-1');
		});
	});

	describe('Smart shelves', () => {
		it('should list, create, and load smart shelf books through mounted routes', async () => {
			mockFetch
				.mockResolvedValueOnce({
					ok: true,
					json: () => Promise.resolve([
						{ id: 'smart-1', name: '继续读', filter_criteria: { reading_status: 'reading' } }
					])
				})
				.mockResolvedValueOnce({
					ok: true,
					json: () => Promise.resolve({ id: 'smart-2', name: '已读完', filter_criteria: { reading_status: 'completed' } })
				})
				.mockResolvedValueOnce({
					ok: true,
					json: () => Promise.resolve({
						data: [{ id: 'book-1', title: '凡人修仙传', author: '忘语', cover_path: null }]
					})
				});

			const shelves = await api.getSmartShelves();
			const created = await api.createSmartShelf({
				name: '已读完',
				description: '回顾用',
				filter_criteria: { reading_status: 'completed' }
			});
			const books = await api.getSmartShelfBooks('smart-2');

			expect(mockFetch).toHaveBeenNthCalledWith(1, '/api/shelves/smart', expect.anything());
			expect(mockFetch).toHaveBeenNthCalledWith(
				2,
				'/api/shelves/smart',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({
						name: '已读完',
						description: '回顾用',
						filter_criteria: { reading_status: 'completed' }
					})
				})
			);
			expect(mockFetch).toHaveBeenNthCalledWith(3, '/api/shelves/smart/smart-2/books', expect.anything());
			expect(shelves[0].filter_criteria.reading_status).toBe('reading');
			expect(created.name).toBe('已读完');
			expect(books[0].title).toBe('凡人修仙传');
		});

		it('should reorder manual shelves through the shared API wrapper', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({ status: 'ok' })
			});

			await api.reorderShelf('shelf-1', ['book-2', 'book-1']);

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/shelves/shelf-1/reorder',
				expect.objectContaining({
					method: 'PUT',
					body: JSON.stringify({ book_ids: ['book-2', 'book-1'] })
				})
			);
		});
	});

	describe('AI chat', () => {
		it('should use the streaming chat endpoint when returning a stream', async () => {
			const encoder = new TextEncoder();
			const stream = new ReadableStream<Uint8Array>({
				start(controller) {
					controller.enqueue(encoder.encode('data: 你好\n\ndata: [DONE]\n\n'));
					controller.close();
				}
			});
			mockFetch.mockResolvedValueOnce({
				ok: true,
				body: stream
			});

			const tokens: string[] = [];
			await api.streamAiChat(
				[{ role: 'user', content: '帮我续写这一段' }],
				(token) => tokens.push(token),
				{ book_id: 'book-1', include_rag: true }
			);

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/ai/chat/stream',
				expect.objectContaining({
					method: 'POST',
					credentials: 'include',
					body: JSON.stringify({
						messages: [{ role: 'user', content: '帮我续写这一段' }],
						book_id: 'book-1',
						include_rag: true
					})
				})
			);
			expect(tokens).toEqual(['你好']);
		});

		it('should route reading companion chat through registered chat endpoint', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({ message: { role: 'assistant', content: '继续读，不剧透。' } })
			});

			const result = await api.chatCompanion({
				book_id: 'book-1',
				message: '这一章发生了什么？',
				system_prompt: 'No spoilers',
				context: { current_chapter: 2 }
			});

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/ai/chat',
				expect.objectContaining({
					method: 'POST',
					body: expect.stringContaining('"include_rag":true')
				})
			);
			expect(result.content).toBe('继续读，不剧透。');
		});
	});

	describe('AI utility workflows', () => {
		it('should summarize text through the mounted AI summarize route', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({ summary: '主角完成试炼。', key_points: ['试炼', '突破'] })
			});

			const result = await api.aiSummarize('第一章内容', 'bullet_points');

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/ai/summarize',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({ text: '第一章内容', style: 'bullet_points' })
				})
			);
			expect(result.key_points).toEqual(['试炼', '突破']);
		});

		it('should extract entities with optional book ACL context', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({
					entities: [{ name: '萧炎', entity_type: 'person', description: '主角', aliases: [] }],
					relationships: []
				})
			});

			const result = await api.aiExtractEntities('萧炎出场', 'book-1');

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/ai/extract-entities',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({ text: '萧炎出场', book_id: 'book-1' })
				})
			);
			expect(result.entities[0].name).toBe('萧炎');
		});

		it('should suggest tags from title, description, and content sample', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({ genres: ['玄幻'], tags: ['成长'], themes: ['复仇'] })
			});

			const result = await api.aiSuggestTags('斗破苍穹', '少年成长', '三十年河东');

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/ai/suggest-tags',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({
						title: '斗破苍穹',
						description: '少年成长',
						content_sample: '三十年河东'
					})
				})
			);
			expect(result.tags).toEqual(['成长']);
		});

		it('should extract glossary terms with the backend bilingual-pairs contract', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({
					terms: [{ source: '斗气', target: 'Dou Qi', category: 'concept', context: '修炼体系' }]
				})
			});

			const result = await api.extractGlossary([{ source: '斗气大陆', target: 'Dou Qi Continent' }], 'zh', 'en');

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/ai/extract-glossary',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({
						pairs: [{ source: '斗气大陆', target: 'Dou Qi Continent' }],
						source_lang: 'zh',
						target_lang: 'en'
					})
				})
			);
			expect(result.terms[0].target).toBe('Dou Qi');
		});

		it('should call book-level plot diagnostics without stale chapter payloads', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({ issues: [], consistency_score: 100, summary: '暂无问题' })
			});

			const result = await api.detectPlotHoles('book-1');

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/ai/detect-plot-holes',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({ book_id: 'book-1' })
				})
			);
			expect(result.consistency_score).toBe(100);
		});

		it('should generate chapter titles for one addressed chapter', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({
					titles: [{ title: '风起乌坦城', style: '意境' }],
					recommended: 0
				})
			});

			const result = await api.generateChapterTitles('book-1', 3);

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/ai/generate-chapter-titles',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({ book_id: 'book-1', chapter_index: 3 })
				})
			);
			expect(result.titles[0].title).toBe('风起乌坦城');
		});

		it('should cleanup forum text through the mounted AI route', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({ cleaned_text: '正文', removed_count: 2 })
			});

			const result = await api.cleanupForumText('广告\n正文');

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/ai/cleanup-forum-text',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({ text: '广告\n正文' })
				})
			);
			expect(result.cleaned_text).toBe('正文');
		});

		it('should normalize sparse community summary responses', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({ status: 'no_unsummarized_communities', book_id: 'book-1' })
			});

			const result = await api.summarizeCommunities('book-1');

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/entities/communities/book-1/summarize',
				expect.objectContaining({ method: 'POST' })
			);
			expect(result.summarized).toBe(0);
			expect(result.errors).toEqual([]);
		});
	});

	describe('File upload', () => {
		it('should unwrap the upload task from an API envelope', async () => {
			const task = { id: 'task-1', status: 'queued', kind: 'book_upload' };
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({
					code: 0,
					message: 'ok',
					timestamp: '2026-06-13T00:00:00Z',
					data: task
				})
			});

			const file = new File(['hello'], 'novel.epub', { type: 'application/epub+zip' });
			const result = await api.uploadBook(file, 'lib-1');

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/books/upload',
				expect.objectContaining({
					method: 'POST',
					credentials: 'include',
					body: expect.any(FormData)
				})
			);
			expect(result).toEqual(task);
		});
	});

	describe('Duplicate detection workbench', () => {
		it('starts a duplicate scan with the selected scope and semantic option', async () => {
			const scan = {
				id: 'scan-1',
				status: 'queued',
				progress: 0,
				books_total: 200,
				books_processed: 0,
				chapters_processed: 0,
				candidates_found: 0,
				pairs_found: 0,
				exact_pairs: 0,
				contained_pairs: 0,
				semantic_pairs: 0,
				error_message: null,
				progress_message: '等待扫描',
			};
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(scan),
			});

			const result = await api.startDuplicateScan({
				library_id: 'library-1',
				include_semantic: true,
			});

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/duplicates/scans',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({ library_id: 'library-1', include_semantic: true }),
				}),
			);
			expect(result).toEqual(scan);
		});

		it('lists duplicate pairs using relation, status, and library filters', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({ items: [], total: 0, limit: 50, offset: 100 }),
			});

			const result = await api.getDuplicatePairs({
				candidate_kind: 'content',
				relation: 'contained_version',
				status: 'pending',
				library_id: 'library-1',
				limit: 50,
				offset: 100,
			});

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/duplicates?candidate_kind=content&relation=contained_version&status=pending&library_id=library-1&limit=50&offset=100',
				expect.anything(),
			);
			expect(result).toEqual({ items: [], total: 0, limit: 50, offset: 100 });
		});

		it('lists exact-file import discoveries with scope and pagination', async () => {
			const page = { items: [], total: 0, limit: 5, offset: 10 };
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(page),
			});

			const result = await api.getExactFileDiscoveries({
				library_id: 'library-1',
				limit: 5,
				offset: 10,
			});

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/duplicates/exact-file-discoveries?library_id=library-1&limit=5&offset=10',
				expect.anything(),
			);
			expect(result).toEqual(page);
		});

		it('fetches the latest duplicate scan for polling', async () => {
			const scan = {
				id: 'scan-1',
				status: 'running',
				progress: 42,
				books_total: 200,
				books_processed: 84,
				chapters_processed: 3200,
				candidates_found: 21,
				pairs_found: 8,
				exact_pairs: 2,
				contained_pairs: 4,
				semantic_pairs: 2,
				error_message: null,
				progress_message: '正在比对章节',
			};
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(scan),
			});

			const result = await api.getLatestDuplicateScan();

			expect(mockFetch).toHaveBeenCalledWith('/api/duplicates/scans/latest', expect.anything());
			expect(result).toEqual(scan);
		});

		it('fetches chapter-level evidence for one duplicate pair', async () => {
			const detail = { id: 'pair-1', chapter_matches: [] };
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(detail),
			});

			const result = await api.getDuplicatePair('pair-1');

			expect(mockFetch).toHaveBeenCalledWith('/api/duplicates/pair-1', expect.anything());
			expect(result).toEqual(detail);
		});

		it('resolves a duplicate pair with one of the supported review actions', async () => {
			const resolved = { id: 'pair-1', status: 'resolved' };
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(resolved),
			});

			const result = await api.resolveDuplicatePair('pair-1', { action: 'keep_b' });

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/duplicates/pair-1/resolve',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({ action: 'keep_b' }),
				}),
			);
			expect(result).toEqual(resolved);
		});

		it('loads text diff for one chapter match only when requested', async () => {
			const diff = {
				changes: [
					{ tag: 'equal', value: '风雪敲打窗棂。' },
					{ tag: 'insert', value: '他终于回来了。' },
				],
				ratio: 0.94,
				truncated: false,
			};
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(diff),
			});

			const result = await api.getDuplicateMatchDiff('pair-1', 'match-1');

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/duplicates/pair-1/matches/match-1/diff',
				expect.anything(),
			);
			expect(result).toEqual(diff);
		});
	});

	describe('Historical endpoint contracts', () => {
		it('should not expose helpers for unmounted or unregistered backend routes', async () => {
			for (const methodName of [
				'emptyLibraryTrash',
				'getDuplicates',
				'findDuplicates',
				'startLibraryScan',
				'getAnnotation',
				'importFromCalibre',
				'importFromOpds',
				'shareCollection',
				'getSharedCollection',
				'exportCollection',
				'triggerLibraryScan',
			]) {
				expect(methodName in api).toBe(false);
			}

			expect(mockFetch).not.toHaveBeenCalled();
		});

		it('should not call unmounted historical route URLs from the API client', () => {
			const source = readFileSync(resolve(process.cwd(), 'src/lib/services/api.ts'), 'utf8');

			for (const pattern of [
				'/calibre/',
				'/opds',
				'/import-sources',
				'/import/calibre',
				'/import/opds',
				'/admin/scan',
				'/empty-trash',
				'/collections/${id}/share',
				'/collections/${id}/export',
			]) {
				expect(source).not.toContain(pattern);
			}
		});

		it('should convert backend annotation export JSON into a Blob', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({
					code: 200,
					message: 'SUCCESS',
					timestamp: 1770768000,
					data: { content: '# Annotations\n\n> saved line\n', format: 'markdown' },
				})
			});

			const result = await api.exportAnnotations('book-1', 'markdown');

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/books/book-1/annotations/export?format=markdown',
				expect.objectContaining({ credentials: 'include' })
			);
			expect(result).toBeInstanceOf(Blob);
			expect(result.type).toBe('text/markdown;charset=utf-8');
			await expect(result.text()).resolves.toContain('saved line');
		});
	});

	describe('Tasks', () => {
		it('should fetch tasks filtered by book and category', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({ data: [], total: 0, page: 1, per_page: 10 })
			});

			await api.getTaskQueue({ book_id: 'book-1', category: 'ai', page: 1, per_page: 10 });

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/tasks?book_id=book-1&category=ai&page=1&per_page=10',
				expect.anything()
			);
		});
	});

	describe('Semantic tags', () => {
		it('should flatten heatmap contract into rows for the UI', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({
					book_id: 'book-1',
					total_chapters: 2,
					profiles: [
						{ id: 'profile-1', name: '热血', color: '#ef4444' },
						{ id: 'profile-2', name: '悬疑', color: '#8b5cf6' }
					],
					scores: [
						{ chapter_index: 0, tag_profile_id: 'profile-1', score: 0.72, top_chunk_score: 0.91 },
						{ chapter_index: 1, tag_profile_id: 'profile-2', score: 0.35, top_chunk_score: 0.44 }
					]
				})
			});

			const result = await api.getBookHeatmap('book-1');

			expect(mockFetch).toHaveBeenCalledWith('/api/semantic-tags/books/book-1/heatmap', expect.anything());
			expect(result).toEqual([
				{
					chapter_index: 0,
					tag_profile_id: 'profile-1',
					name: '热血',
					color: '#ef4444',
					avg_score: 0.72,
					max_score: 0.91,
					match_count: 1
				},
				{
					chapter_index: 1,
					tag_profile_id: 'profile-2',
					name: '悬疑',
					color: '#8b5cf6',
					avg_score: 0.35,
					max_score: 0.44,
					match_count: 1
				}
			]);
		});

		it('should normalize book markers into a stable frontend contract', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve([
					{
						id: 'marker-1',
						tag_profile_id: 'profile-1',
						chapter_index: 4,
						chunk_index: 2,
						similarity_score: 0.87,
						content_snippet: '风雪压城，少年拔剑而起。',
						char_offset: 128,
						created_at: '2026-06-18T00:00:00Z'
					}
				])
			});

			const result = await api.getBookMarkers('book-1');

			expect(mockFetch).toHaveBeenCalledWith('/api/semantic-tags/books/book-1/markers', expect.anything());
			expect(result).toEqual([
				expect.objectContaining({
					id: 'marker-1',
					tag_profile_id: 'profile-1',
					profile_id: 'profile-1',
					chapter_index: 4,
					chunk_index: 2,
					similarity_score: 0.87,
					score: 0.87,
					content_snippet: '风雪压城，少年拔剑而起。',
					snippet: '风雪压城，少年拔剑而起。',
					char_offset: 128,
					offset: 128
				})
			]);
		});

		it('should fetch the semantic overview from the mounted route', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({
					total_profiles: 6,
					total_books_tagged: 12,
					categories: { trope: 3, emotion: 2, warning: 1 }
				})
			});

			const result = await api.getSemanticOverview();

			expect(mockFetch).toHaveBeenCalledWith('/api/semantic-tags/overview', expect.anything());
			expect(result.total_profiles).toBe(6);
			expect(result.categories.warning).toBe(1);
		});
	});

	describe('Error handling', () => {
		it('should handle network errors gracefully', async () => {
			vi.useFakeTimers();
			try {
				mockFetch.mockRejectedValue(new TypeError('Failed to fetch'));
				const promise = api.getBooks().then(
					() => undefined,
					(error: unknown) => error
				);
				// Advance through retry delays
				await vi.advanceTimersByTimeAsync(20000);
				const error = await promise;
				expect(error).toBeInstanceOf(Error);
				expect((error as Error).message).toBe('网络连接失败');
			} finally {
				vi.useRealTimers();
			}
		});

		it('should handle malformed JSON error response', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: false,
				status: 500,
				statusText: 'Internal Server Error',
				json: () => Promise.reject(new Error('not JSON'))
			});

			await expect(api.getBooks()).rejects.toThrow('Internal Server Error');
		});
	});

	describe('Admin API', () => {
		it('should fetch users list', async () => {
			const mockUsers = [{ id: 'u1', username: 'admin', display_name: 'Admin', books_count: 42 }];
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(mockUsers)
			});

			const result = await api.getUsers();
			expect(mockFetch).toHaveBeenCalledWith('/api/admin/users', expect.anything());
			expect(result).toHaveLength(1);
			expect(result[0].username).toBe('admin');
		});

		it('should delete a user', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(null)
			});

			await api.deleteUser('user-123');
			expect(mockFetch).toHaveBeenCalledWith(
				'/api/admin/users/user-123',
				expect.objectContaining({ method: 'DELETE' })
			);
		});

		it('should create, update, and delete user groups', async () => {
			mockFetch
				.mockResolvedValueOnce({ ok: true, json: () => Promise.resolve({ id: 'grp-1', name: '家庭', color: 'sky' }) })
				.mockResolvedValueOnce({ ok: true, json: () => Promise.resolve({ status: 'ok' }) })
				.mockResolvedValueOnce({ ok: true, json: () => Promise.resolve({ status: 'deleted' }) });

			await api.createGroup({ name: '家庭', description: '家庭成员', color: 'sky' });
			await api.updateGroup('grp-1', { name: '核心家庭', description: '常用账号', color: 'emerald' });
			await api.deleteGroup('grp-1');

			expect(mockFetch).toHaveBeenNthCalledWith(
				1,
				'/api/admin/groups',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify({ name: '家庭', description: '家庭成员', color: 'sky' })
				})
			);
			expect(mockFetch).toHaveBeenNthCalledWith(
				2,
				'/api/admin/groups/grp-1',
				expect.objectContaining({
					method: 'PATCH',
					body: JSON.stringify({ name: '核心家庭', description: '常用账号', color: 'emerald' })
				})
			);
			expect(mockFetch).toHaveBeenNthCalledWith(
				3,
				'/api/admin/groups/grp-1',
				expect.objectContaining({ method: 'DELETE' })
			);
		});

		it('should fetch system logs with filters', async () => {
			const mockLogs = [{ timestamp: '2024-01-01T00:00:00Z', level: 'error', target: 'nova_api', message: 'oops' }];
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(mockLogs)
			});

			const result = await api.getSystemLogs({ level: 'error', limit: 10 });
			expect(mockFetch).toHaveBeenCalledWith(
				'/api/admin/logs?level=error&limit=10',
				expect.anything()
			);
			expect(result[0].level).toBe('error');
		});

		it('should fetch scheduled jobs', async () => {
			const mockJobs = [{ id: 'j1', name: 'Library Scan', cron: '0 * * * *', status: 'active' }];
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(mockJobs)
			});

			const result = await api.getScheduledJobs();
			expect(mockFetch).toHaveBeenCalledWith('/api/admin/jobs', expect.anything());
			expect(result[0].name).toBe('Library Scan');
		});

		it('should toggle a job', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(null)
			});

			await api.toggleJob('job-1', false);
			expect(mockFetch).toHaveBeenCalledWith(
				'/api/admin/jobs/job-1',
				expect.objectContaining({
					method: 'PATCH',
					body: JSON.stringify({ enabled: false })
				})
			);
		});

		it('should fetch permission templates', async () => {
			const mockTemplates = [
				{ id: 'tpl-read', name: '只读', can_read: true, can_write: false, can_manage: false, is_system: true }
			];
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve(mockTemplates)
			});

			const result = await api.getPermissionTemplates();

			expect(mockFetch).toHaveBeenCalledWith('/api/admin/permission-templates', expect.anything());
			expect(result[0].name).toBe('只读');
		});

		it('should create a permission template', async () => {
			const payload = { name: '编辑协作', description: '可读写不可管理', can_read: true, can_write: true, can_manage: false };
			mockFetch.mockResolvedValueOnce({
				ok: true,
				json: () => Promise.resolve({ id: 'tpl-1', ...payload, is_system: false })
			});

			await api.createPermissionTemplate(payload);

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/admin/permission-templates',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify(payload)
				})
			);
		});

		it('should update and delete custom permission templates', async () => {
			mockFetch
				.mockResolvedValueOnce({ ok: true, json: () => Promise.resolve({ status: 'ok' }) })
				.mockResolvedValueOnce({ ok: true, json: () => Promise.resolve({ status: 'deleted' }) });

			await api.updatePermissionTemplate('tpl-1', { name: '协作者', can_write: true });
			await api.deletePermissionTemplate('tpl-1');

			expect(mockFetch).toHaveBeenNthCalledWith(
				1,
				'/api/admin/permission-templates/tpl-1',
				expect.objectContaining({
					method: 'PATCH',
					body: JSON.stringify({ name: '协作者', can_write: true })
				})
			);
			expect(mockFetch).toHaveBeenNthCalledWith(
				2,
				'/api/admin/permission-templates/tpl-1',
				expect.objectContaining({ method: 'DELETE' })
			);
		});
	});
});
