import type {
	Book, Chapter, Library, Entity, GlossaryEntry,
	Task, SearchQuery, SearchResult, PaginatedResponse,
	Annotation, Bookmark, ReadingProgress, ReadingStats, Collection,
	Translation, Series, SeriesMetadata, Person, PersonBook, AppSettings, ReadingStatus,
	CreateGlossaryEntryInput,
	AiExtractEntitiesResult, AiSuggestedTagsResult, AiSummaryResult,
	ChapterTitleResult, CleanupForumTextResult, CommunitySummaryResult,
	GlossaryExtractionResult, PlotHoleReport, TranslateTextResult,
	SemanticProfile, TagScore, TagHeatmapEntry, TagMarker,
	SemanticOverview, BookRadarResult, VibeSearchResult, VibeBookmark,
	AppNotification, LibraryFeatures, LibraryPermissionsResponse,
	PermissionTemplate, CrossBookSearchResponse, SmartShelf, SmartShelfBook,
	DuplicateScan, StartDuplicateScanInput,
	DuplicatePairDetail, DuplicatePairFilters, DuplicatePairPage, DuplicateResolutionAction,
	DuplicateResolutionResult,
	DuplicateMatchDiff,
	ExactFileDiscoveryPage,
} from '$types/models';

const BASE_URL = '/api';

/** Default request timeout in milliseconds (30s). */
const REQUEST_TIMEOUT_MS = 30_000;

/** Max retries for retryable errors (429, 503, network). */
const MAX_RETRIES = 3;

/** Structured API error with status and retryable flag. */
export class ApiRequestError extends Error {
	constructor(
		public readonly status: number,
		message: string,
		public readonly retryable: boolean = false,
	) {
		super(message);
		this.name = 'ApiRequestError';
	}

	get isUnauthorized(): boolean {
		return this.status === 401;
	}

	get isNotFound(): boolean {
		return this.status === 404;
	}
}

class ApiClient {
	/** Unwrap backend responses that wrap arrays in { data: [...] } */
	private unwrapArray<T>(result: unknown): T[] {
		if (Array.isArray(result)) return result;
		if (result && typeof result === 'object' && 'data' in result) {
			const wrapped = result as { data: unknown };
			if (Array.isArray(wrapped.data)) return wrapped.data;
		}
		return [];
	}

	private _refreshing: Promise<boolean> | null = null;

	/** Try to refresh the access token. Returns true if successful. */
	private async _tryRefresh(): Promise<boolean> {
		if (this._refreshing) return this._refreshing;
		this._refreshing = (async () => {
			try {
				const res = await fetch(`${BASE_URL}/auth/refresh`, {
					method: 'POST',
					credentials: 'include',
					headers: { 'Content-Type': 'application/json' },
					body: '{}',
				});
				return res.ok;
			} catch {
				return false;
			} finally {
				this._refreshing = null;
			}
		})();
		return this._refreshing;
	}

	private async request<T>(path: string, options?: RequestInit & { retries?: number; noRetry?: boolean; _refreshed?: boolean }): Promise<T> {
		const { retries = 0, noRetry = false, _refreshed = false, ...fetchOptions } = options ?? {};

		const controller = new AbortController();
		const timeout = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);

		try {
			const response = await fetch(`${BASE_URL}${path}`, {
				credentials: 'include',
				headers: {
					'Content-Type': 'application/json',
					...fetchOptions?.headers,
				},
				signal: controller.signal,
				...fetchOptions,
			});

			if (!response.ok) {
				const body = await response.json().catch(() => null);
				// Support both envelope format and legacy format
				const message = body?.message || body?.error?.message || response.statusText;
				const retryable = body?.error?.retryable ?? (response.status === 429 || response.status === 503);

				// Auto-refresh on 401 (skip for auth endpoints to avoid loops)
				if (response.status === 401 && !_refreshed && !path.startsWith('/auth/')) {
					const refreshed = await this._tryRefresh();
					if (refreshed) {
						return this.request<T>(path, { ...options, _refreshed: true });
					}
				}

				const error = new ApiRequestError(response.status, message, retryable);

				// Retry if retryable and we haven't exceeded max retries
				if (retryable && !noRetry && retries < MAX_RETRIES) {
					const delay = Math.min(1000 * 2 ** retries, 8000) + Math.random() * 500;
					await new Promise(resolve => setTimeout(resolve, delay));
					return this.request<T>(path, { ...options, retries: retries + 1 });
				}

				throw error;
			}

			const body = await response.json();

			// Unwrap envelope format: { code, message, timestamp, data }
			if (body && typeof body === 'object' && 'code' in body && 'data' in body && 'timestamp' in body) {
				return body.data as T;
			}

			// Fallback: return raw body (for endpoints not yet wrapped)
			return body as T;
		} catch (e) {
			if (e instanceof ApiRequestError) throw e;

			// AbortController timeout
			if (e instanceof DOMException && e.name === 'AbortError') {
				const error = new ApiRequestError(0, '请求超时，请检查网络连接', true);
				if (!noRetry && retries < MAX_RETRIES) {
					const delay = Math.min(1000 * 2 ** retries, 8000) + Math.random() * 500;
					await new Promise(resolve => setTimeout(resolve, delay));
					return this.request<T>(path, { ...options, retries: retries + 1 });
				}
				throw error;
			}

			// Network errors (offline, DNS failure, etc.)
			if (e instanceof TypeError && e.message.includes('fetch')) {
				const error = new ApiRequestError(0, '网络连接失败', true);
				if (!noRetry && retries < MAX_RETRIES) {
					const delay = Math.min(1000 * 2 ** retries, 8000) + Math.random() * 500;
					await new Promise(resolve => setTimeout(resolve, delay));
					return this.request<T>(path, { ...options, retries: retries + 1 });
				}
				throw error;
			}

			throw e;
		} finally {
			clearTimeout(timeout);
		}
	}

	// ─── Generic HTTP Helpers ─────────────────────────────────────────────

	async get<T = unknown>(path: string): Promise<T> {
		return this.request<T>(path);
	}

	async post<T = unknown>(path: string, data?: unknown): Promise<T> {
		return this.request<T>(path, {
			method: 'POST',
			body: data != null ? JSON.stringify(data) : undefined,
		});
	}

	async put<T = unknown>(path: string, data?: unknown): Promise<T> {
		return this.request<T>(path, {
			method: 'PUT',
			body: data != null ? JSON.stringify(data) : undefined,
		});
	}

	async del<T = unknown>(path: string): Promise<T> {
		return this.request<T>(path, { method: 'DELETE' });
	}

	// Books
	async getBooks(params?: {
		page?: number;
		per_page?: number;
		sort_by?: string;
		status?: string;
		reading_status?: string;
		language?: string;
		format?: string;
		search?: string;
		library_id?: string;
		series_id?: string;
	}): Promise<PaginatedResponse<Book>> {
		const query = new URLSearchParams();
		if (params) {
			Object.entries(params).forEach(([k, v]) => {
				if (v != null) query.set(k, String(v));
			});
		}
		return this.request(`/books?${query}`);
	}

	async getBook(id: string): Promise<Book> {
		return this.request(`/books/${id}`);
	}

	async updateBook(id: string, data: Partial<Book>): Promise<Book> {
		return this.request(`/books/${id}`, {
			method: 'PUT',
			body: JSON.stringify(data),
		});
	}

	async updateBookReadingStatus(id: string, readingStatus: ReadingStatus): Promise<Book> {
		return this.request(`/books/${id}`, {
			method: 'PUT',
			body: JSON.stringify({ reading_status: readingStatus }),
		});
	}

	async deleteBook(id: string): Promise<void> {
		await this.request(`/books/${id}`, { method: 'DELETE' });
	}

	async reprocessBook(id: string): Promise<Task> {
		return this.request(`/books/${id}/reprocess`, { method: 'POST' });
	}

	async getBookTags(): Promise<Array<{ tag: string; count: number }>> {
		return this.request('/books/tags');
	}

	// Chapters
	async getChapters(bookId: string): Promise<Chapter[]> {
		const result = await this.request(`/books/${bookId}/chapters`);
		return this.unwrapArray<Chapter>(result);
	}

	async getChapter(bookId: string, index: number): Promise<Chapter> {
		return this.request(`/books/${bookId}/chapters/${index}`);
	}

	async getChapterContent(bookId: string, index: number): Promise<{ content: string; entities: Array<{ start: number; end: number; type: string; name: string; id: string }> }> {
		const [chapterData, entitiesData] = await Promise.all([
			this.request<{ content: string }>(`/books/${bookId}/chapters/${index}/content`),
			this.request<{ data: Array<{ start_offset: number; end_offset: number; type?: string; entity_type?: string; name: string; id: string }> }>(`/books/${bookId}/chapters/${index}/entities`).catch(() => ({ data: [] })),
		]);
		return {
			content: chapterData?.content ?? '',
			entities: (entitiesData?.data ?? []).map((e) => ({
				start: e.start_offset,
				end: e.end_offset,
				type: e.type ?? e.entity_type ?? 'unknown',
				name: e.name,
				id: e.id,
			})),
		};
	}

	async getChapterEntities(bookId: string, index: number): Promise<Array<{ start: number; end: number; type: string; name: string; id: string }>> {
		const result = await this.request<{ data?: Array<{ start_offset: number; end_offset: number; type?: string; entity_type?: string; name: string; id: string }> }>(
			`/books/${bookId}/chapters/${index}/entities`
		);
		return (result.data ?? []).map((e) => ({
			start: e.start_offset,
			end: e.end_offset,
			type: e.type ?? e.entity_type ?? 'unknown',
			name: e.name,
			id: e.id,
		}));
	}

	// Libraries
	async getLibraries(): Promise<Library[]> {
		const result = await this.request('/libraries');
		return this.unwrapArray<Library>(result);
	}

	async getLibrary(id: string): Promise<Library> {
		return this.request(`/libraries/${id}`);
	}

	async createLibrary(data: {
		name: string;
		root_path: string;
		description?: string;
		auto_scan?: boolean;
		scan_interval_secs?: number;
		include_extensions?: string[];
		exclude_patterns?: string[];
		compute_hashes?: boolean;
	}): Promise<Library> {
		return this.request('/libraries', {
			method: 'POST',
			body: JSON.stringify(data),
		});
	}

	async updateLibrary(id: string, data: Partial<Library>): Promise<Library> {
		return this.request(`/libraries/${id}`, {
			method: 'PUT',
			body: JSON.stringify(data),
		});
	}

	async deleteLibrary(id: string): Promise<void> {
		await this.request(`/libraries/${id}`, { method: 'DELETE' });
	}

	async getLibraryFeatures(id: string): Promise<LibraryFeatures> {
		return this.request(`/libraries/${id}/features`);
	}

	async setLibraryFeatures(id: string, features: Partial<LibraryFeatures>): Promise<LibraryFeatures> {
		return this.request(`/libraries/${id}/features`, {
			method: 'PUT',
			body: JSON.stringify(features),
		});
	}

	async getLibraryPermissions(id: string): Promise<LibraryPermissionsResponse> {
		const resp = await this.request<Partial<LibraryPermissionsResponse>>(`/libraries/${id}/permissions`);
		return {
			permissions: resp.permissions ?? [],
			group_permissions: resp.group_permissions ?? [],
		};
	}

	async setLibraryPermissions(id: string, permissions: LibraryPermissionsResponse): Promise<{ status: string }> {
		return this.request(`/libraries/${id}/permissions`, {
			method: 'PUT',
			body: JSON.stringify(permissions),
		});
	}

	async scanLibrary(id: string): Promise<{ message: string; new_books?: number; skipped_duplicates?: number; series_detected?: number; errors?: number }> {
		return this.request(`/libraries/${id}/scan`, { method: 'POST' });
	}

	async getLibraryScanStatus(id: string): Promise<{
		status: 'idle' | 'scanning' | 'processing' | 'complete' | 'error';
		total_files: number;
		processed_files: number;
		new_books: number;
		errors: string[];
		started_at?: string;
		elapsed_seconds?: number;
	}> {
		return this.request(`/libraries/${id}/scan-status`);
	}

	async runLibraryMaintenance(
		id: string,
		action: 'reindex' | 'cleanup-orphan-covers' | 'recompute-hashes',
	): Promise<{ message: string; library_id: string; action: string; task_id: string }> {
		return this.request(`/libraries/${id}/maintenance/${action}`, { method: 'POST' });
	}

	async analyzeLibrary(id: string): Promise<{ message: string; tasks_queued?: number; total_unanalyzed?: number }> {
		return this.request(`/libraries/${id}/analyze`, { method: 'POST' });
	}

	async getLibrarySeries(id: string): Promise<Series[]> {
		return this.request(`/libraries/${id}/series`);
	}

	async getSeriesByLibrary(libraryId: string): Promise<Series[]> {
		const result = await this.request(`/libraries/${libraryId}/series`);
		return this.unwrapArray(result);
	}

	// Search
	async search(query: SearchQuery): Promise<{ results: SearchResult[]; total: number; took_ms: number; intent?: string; rerank_applied?: boolean; timing?: { total_ms: number; bm25_ms: number; vector_ms: number; rerank_ms: number } }> {
		return this.request('/search', {
			method: 'POST',
			body: JSON.stringify(query),
		});
	}

	async searchFacets(q: string): Promise<{ facets: { format: Array<{value: string; count: number}>; author: Array<{value: string; count: number}> } }> {
		return this.request(`/search/facets?q=${encodeURIComponent(q)}`);
	}

	async searchSuggest(q: string, limit = 5): Promise<{ suggestions: Array<{text: string; type: string}> }> {
		return this.request(`/search/suggest?q=${encodeURIComponent(q)}&limit=${limit}`);
	}

	async findSimilar(chunkId: string, limit?: number): Promise<SearchResult[]> {
		return this.request(`/search/similar/${chunkId}?limit=${limit ?? 10}`);
	}

	async getChunkContext(chunkId: string, window = 2): Promise<{ chunk_id: string; context: Array<{ chunk_id: string; chunk_index: number; content: string; is_target: boolean }> }> {
		return this.request(`/search/context/${chunkId}?window=${window}`);
	}

	async searchGraph(query: string, bookId?: string): Promise<{ results: SearchResult[]; paths: Array<{ source: string; target: string; nodes: string[]; relationships: string[]; length?: number; path_score?: number; rank_reason?: string; explanation?: string }>; entities: string[]; related_entities: string[]; timing: { total_ms: number } }> {
		return this.request('/search/graph', {
			method: 'POST',
			body: JSON.stringify({ query, book_id: bookId }),
		});
	}

	async searchCrossBook(query: string, bookIds?: string[], limit = 30): Promise<CrossBookSearchResponse> {
		return this.request('/search/cross-book', {
			method: 'POST',
			body: JSON.stringify({ query, book_ids: bookIds ?? [], limit, group_by_book: true }),
		});
	}

	async searchGlobal(query: string, bookId?: string, level?: number): Promise<{ answer: string; sources: Array<{ community_id: string; summary: string; members: string[] }>; communities_analyzed: number; timing: { total_ms: number } }> {
		return this.request('/search/global', {
			method: 'POST',
			body: JSON.stringify({ query, book_id: bookId, level: level ?? 1 }),
		});
	}

	async summarizeCommunities(bookId: string): Promise<CommunitySummaryResult> {
		const result = await this.request<Partial<CommunitySummaryResult>>(`/entities/communities/${bookId}/summarize`, {
			method: 'POST',
		});
		return {
			status: result.status ?? 'unknown',
			book_id: result.book_id,
			total_communities: result.total_communities ?? 0,
			summarized: result.summarized ?? 0,
			errors: result.errors ?? [],
		};
	}

	// Entities
	async getEntities(params?: {
		type?: string;
		book_id?: string;
		library_id?: string;
		series_id?: string;
		search?: string;
		sort?: string;
		limit?: number;
	}): Promise<Entity[]> {
		const query = new URLSearchParams();
		if (params) {
			Object.entries(params).forEach(([k, v]) => {
				if (v != null) query.set(k, String(v));
			});
		}
		const result = await this.request(`/entities?${query}`);
		return this.unwrapArray<Entity>(result);
	}

	async getEntity(id: string): Promise<Entity> {
		return this.request(`/entities/${id}`);
	}

	async getEntityRelations(id: string): Promise<Array<{ source: string; target: string; type: string; targetName: string }>> {
		return this.request(`/entities/${id}/relations`);
	}

	async getEntityMentions(id: string): Promise<Array<{ book_title: string; chapter_title: string; context: string; chapter_index: number; book_id: string }>> {
		return this.request(`/entities/${id}/mentions`);
	}

	async getEntityGraph(params?: { book_id?: string; library_id?: string; series_id?: string; depth?: number }): Promise<{
		nodes: Array<{ id: string; label: string; type: string; size: number }>;
		edges: Array<{ source: string; target: string; label: string; weight: number }>;
	}> {
		const query = new URLSearchParams();
		if (params) {
			Object.entries(params).forEach(([k, v]) => {
				if (v != null) query.set(k, String(v));
			});
		}
		return this.request(`/entities/graph?${query}`);
	}

	// Glossary
	async getGlossary(params?: { book_id?: string; search?: string }): Promise<GlossaryEntry[]> {
		const query = new URLSearchParams();
		if (params) {
			Object.entries(params).forEach(([k, v]) => {
				if (v != null) query.set(k, String(v));
			});
		}
		const result = await this.request(`/glossary?${query}`);
		return this.unwrapArray<GlossaryEntry>(result);
	}

	async createGlossaryEntry(data: CreateGlossaryEntryInput): Promise<{ id: string; term: string }> {
		return this.request('/glossary', {
			method: 'POST',
			body: JSON.stringify({
				term: data.term,
				definition: data.definition ?? '',
				source_language: data.source_language ?? null,
				target_language: data.target_language ?? null,
				book_id: data.book_id ?? null,
			}),
		});
	}

	// Translation
	async translate(data: {
		text: string;
		source_language: string;
		target_language: string;
		book_id?: string;
		use_glossary?: boolean;
	}): Promise<TranslateTextResult> {
		return this.request('/translate', {
			method: 'POST',
			body: JSON.stringify(data),
		});
	}

	// Reading Progress
	async getReadingProgress(bookId: string): Promise<ReadingProgress> {
		return this.request(`/books/${bookId}/progress`);
	}

	async updateReadingProgress(bookId: string, data: Partial<ReadingProgress> & {
		client_last_read_at?: string;
		force?: boolean;
	}): Promise<{ status: string; server_progress?: ReadingProgress; client_progress?: ReadingProgress; message?: string }> {
		return this.request(`/books/${bookId}/progress`, {
			method: 'PUT',
			body: JSON.stringify(data),
		});
	}

	// ─── Activities & Stats ─────────────────────────────────────
	async getActivities(params?: { limit?: number; offset?: number }): Promise<Array<{
		id: string;
		type: 'reading' | 'annotation' | 'completion' | 'import';
		book_title: string;
		book_id: string;
		chapter_title?: string;
		description: string;
		created_at: string;
		duration_minutes?: number;
		pages_read?: number;
	}>> {
		const query = new URLSearchParams();
		if (params?.limit) query.set('limit', String(params.limit));
		if (params?.offset) query.set('offset', String(params.offset));
		return this.request(`/stats/activities?${query}`);
	}

	async getReadingStats(range: string = 'year'): Promise<{
		totalBooksRead: number;
		totalReadingTime: number;
		totalAnnotations: number;
		avgDailyMinutes: number;
		longestStreak: number;
		currentStreak: number;
		booksThisMonth: number;
		pagesThisWeek: number;
	}> {
		return this.request(`/stats/reading?range=${range}`);
	}

	// Annotations
	async getAnnotations(bookId: string): Promise<Annotation[]> {
		return this.request(`/books/${bookId}/annotations`);
	}

	async createAnnotation(bookId: string, data: Omit<Annotation, 'id' | 'created_at'>): Promise<Annotation> {
		return this.request(`/books/${bookId}/annotations`, {
			method: 'POST',
			body: JSON.stringify(data),
		});
	}

	async deleteAnnotation(bookId: string, annotationId: string): Promise<void> {
		await this.request(`/books/${bookId}/annotations/${annotationId}`, { method: 'DELETE' });
	}

	// Tasks
	async getTasks(params?: { status?: string; limit?: number }): Promise<Task[]> {
		const query = new URLSearchParams();
		if (params) {
			Object.entries(params).forEach(([k, v]) => {
				if (v != null) query.set(k, String(v));
			});
		}
		const result = await this.request(`/tasks?${query}`);
		return this.unwrapArray<Task>(result);
	}

	async cancelTask(id: string): Promise<void> {
		await this.request(`/tasks/${id}/cancel`, { method: 'POST' });
	}

	async retryTask(id: string): Promise<Task> {
		return this.request(`/tasks/${id}/retry`, { method: 'POST' });
	}

	// Task SSE stream
	streamTasks(onMessage: (task: Task) => void): () => void {
		const eventSource = new EventSource(`${BASE_URL}/tasks/stream`);
		eventSource.onmessage = (event) => {
			const task = JSON.parse(event.data);
			onMessage(task);
		};
		return () => eventSource.close();
	}

	// Stats
	async getDashboardStats(): Promise<{
		total_books: number;
		books_in_progress: number;
		reading_time_today_mins: number;
		tasks_running: number;
		storage_used_gb: number;
		entities_extracted: number;
	}> {
		return this.request('/stats/dashboard');
	}

	// Collections
	async getCollections(): Promise<Collection[]> {
		return this.request('/collections');
	}

	async createCollection(data: { name: string; description?: string }): Promise<Collection> {
		return this.request('/collections', {
			method: 'POST',
			body: JSON.stringify(data),
		});
	}

	async getCollection(id: string): Promise<Collection> {
		return this.request(`/collections/${id}`);
	}

	async updateCollection(id: string, data: { name: string; description?: string }): Promise<Collection> {
		return this.request(`/collections/${id}`, {
			method: 'PUT',
			body: JSON.stringify(data),
		});
	}

	async deleteCollection(id: string): Promise<void> {
		return this.request(`/collections/${id}`, { method: 'DELETE' });
	}

	async getCollectionBooks(id: string): Promise<Book[]> {
		return this.request(`/collections/${id}/books`);
	}

	async addBookToCollection(collectionId: string, bookId: string): Promise<void> {
		return this.request(`/collections/${collectionId}/books`, {
			method: 'POST',
			body: JSON.stringify({ book_id: bookId }),
		});
	}

	async removeBookFromCollection(collectionId: string, bookId: string): Promise<void> {
		return this.request(`/collections/${collectionId}/books/${bookId}`, { method: 'DELETE' });
	}

	async chatCompanion(data: {
		book_id: string;
		message: string;
		system_prompt?: string;
		context?: Record<string, unknown>;
	}): Promise<{ content: string }> {
		const messages = [
			...(data.system_prompt ? [{ role: 'system', content: data.system_prompt }] : []),
			{ role: 'user', content: data.message },
		];
		const response = await this.request<{ message: { content: string } }>('/ai/chat', {
			method: 'POST',
			body: JSON.stringify({
				messages,
				book_id: data.book_id,
				include_rag: true,
				...(data.context ?? {}),
			}),
		});
		return { content: response.message?.content ?? '' };
	}

	// File upload
	async uploadBook(file: File, libraryId?: string): Promise<Task> {
		const formData = new FormData();
		formData.append('file', file);
		if (libraryId) formData.append('library_id', libraryId);

		const response = await fetch(`${BASE_URL}/books/upload`, {
			method: 'POST',
			credentials: 'include',
			body: formData,
		});
		if (!response.ok) {
			const error = await response.json().catch(() => ({ message: response.statusText }));
			throw new Error(error.message ?? error.error?.message ?? response.statusText);
		}
		const body = await response.json();
		if (body && typeof body === 'object' && 'code' in body && 'data' in body && 'timestamp' in body) {
			return body.data as Task;
		}
		return body as Task;
	}

	// ─── Authentication ─────────────────────────────────────────
	async login(data: { username: string; password: string }): Promise<{
		access_token: string;
		expires_in: number;
		user: { id: string; username: string; display_name?: string; role?: string };
	}> {
		return this.request('/auth/login', {
			method: 'POST',
			body: JSON.stringify(data),
		});
	}

	async register(data: { username: string; password: string }): Promise<{
		access_token: string;
		expires_in: number;
		user: { id: string; username: string; display_name?: string; role?: string };
	}> {
		return this.request('/auth/register', {
			method: 'POST',
			body: JSON.stringify(data),
		});
	}

	async refreshToken(): Promise<{ access_token: string }> {
		return this.request('/auth/refresh', { method: 'POST', body: '{}' });
	}

	async logout(): Promise<void> {
		await this.request('/auth/logout', { method: 'POST' });
	}

	async getMe(): Promise<{ id: string; username: string; display_name?: string; avatar_path?: string; role?: string }> {
		return this.request('/auth/me');
	}

	async updateProfile(data: { username?: string; email?: string; current_password?: string; new_password?: string }): Promise<void> {
		await this.request('/auth/profile', {
			method: 'PUT',
			body: JSON.stringify(data),
		});
	}

	async changePassword(data: { current_password: string; new_password: string }): Promise<void> {
		await this.request('/auth/change-password', {
			method: 'POST',
			body: JSON.stringify(data),
		});
	}

	async uploadAvatar(file: File): Promise<{ avatar_url: string }> {
		const response = await fetch('/api/auth/avatar', {
			method: 'POST',
			credentials: 'include',
			headers: { 'Content-Type': file.type || 'image/jpeg' },
			body: file,
		});
		if (!response.ok) {
			const body = await response.json().catch(() => null);
			throw new Error(body?.error?.message || body?.message || response.statusText);
		}
		const body = await response.json();
		// Unwrap envelope format
		return body.data ?? body;
	}

	// ─── Series ─────────────────────────────────────────────────
	async getSeriesList(params?: { search?: string; status?: string; library_id?: string; sort_by?: string; sort_dir?: 'asc' | 'desc' }): Promise<Series[]> {
		const query = new URLSearchParams();
		if (params?.search) query.set('search', params.search);
		if (params?.status) query.set('status', params.status);
		if (params?.library_id) query.set('library_id', params.library_id);
		if (params?.sort_by) query.set('sort_by', params.sort_by);
		if (params?.sort_dir) query.set('sort_dir', params.sort_dir);
		const result = await this.request(`/libraries/series?${query}`);
		return this.unwrapArray(result);
	}

	async getSeries(id: string): Promise<Series> {
		return this.request(`/libraries/series/${id}`);
	}

	async getSeriesBooks(seriesId: string): Promise<Book[]> {
		const result = await this.request(`/libraries/series/${seriesId}/books`);
		return this.unwrapArray(result);
	}

	async updateSeriesMetadata(seriesId: string, metadata: SeriesMetadata): Promise<Series> {
		return this.request(`/libraries/series/${seriesId}/metadata`, {
			method: 'PUT',
			body: JSON.stringify(metadata),
		});
	}

	async reorderSeriesBooks(seriesId: string, bookIds: string[]): Promise<{ status: string; updated: number }> {
		return this.request(`/libraries/series/${seriesId}/reorder`, {
			method: 'PUT',
			body: JSON.stringify({ book_ids: bookIds }),
		});
	}

	// ─── Persons ────────────────────────────────────────────────
	async getPersons(params?: { search?: string; role?: string }): Promise<Array<{
		id: string;
		name: string;
		original_name?: string;
		avatar_path?: string;
		roles: string[];
		book_count: number;
		total_word_count: number;
	}>> {
		const query = new URLSearchParams();
		if (params?.search) query.set('search', params.search);
		if (params?.role) query.set('role', params.role);
		return this.request(`/persons?${query}`);
	}

	async getPerson(id: string): Promise<Person> {
		return this.request(`/persons/${id}`);
	}

	async getPersonBooks(personId: string): Promise<PersonBook[]> {
		return this.request(`/persons/${personId}/books`);
	}

	// ─── Reading Analytics ──────────────────────────────────────
	async getReadingHeatmap(params?: { days?: number }): Promise<Array<{
		date: string;
		total_minutes: number;
		total_words: number;
		sessions_count: number;
	}>> {
		const days = params?.days ?? 365;
		return this.request(`/stats/reading/heatmap?days=${days}`);
	}

	async getReadingMemories(month: number, day: number): Promise<Array<{
		book_id: string;
		title: string;
		author: string | null;
		cover_path: string | null;
		read_date: string;
		years_ago: number;
	}>> {
		return this.request(`/stats/reading/memories?month=${month}&day=${day}`);
	}

	async getReadingSessions(params?: { limit?: number }): Promise<Array<{
		id: string;
		book_id: string;
		book_title: string;
		start_chapter: number;
		end_chapter: number;
		words_read: number;
		duration_secs: number;
		started_at: string;
	}>> {
		return this.request(`/stats/reading/sessions?limit=${params?.limit ?? 20}`);
	}

	async getReadingGoals(): Promise<Array<{
		id: string;
		label: string;
		goal_type: string;
		target: number;
		progress: number;
		period: string;
	}>> {
		return this.request('/stats/reading/goals');
	}

	async recordReadingSession(data: {
		book_id: string;
		start_chapter: number;
		end_chapter: number;
		words_read: number;
		duration_secs: number;
	}): Promise<void> {
		await this.request('/stats/reading/sessions', {
			method: 'POST',
			body: JSON.stringify(data),
		});
	}

	// ─── Recommendations ────────────────────────────────────────
	async getSimilarBooks(bookId: string): Promise<Book[]> {
		return this.request(`/books/${bookId}/similar`);
	}

	async getRecommendations(): Promise<Array<{
		id: string;
		category: string;
		reason: string;
		books: Array<{
			id: string;
			title: string;
			author: string | null;
			cover_path: string | null;
			score: number;
			match_reason: string;
			semantic_anchors?: string[];
			semantic_anchor_count?: number;
			similarity_score?: number;
			recommendation_score?: number;
		}>;
	}>> {
		return this.request('/recommendations');
	}

	async getReadingQueue(): Promise<{
		queue: Array<{
			id: string;
			title: string;
			author: string | null;
			reading_status: string;
			priority: 'high' | 'medium' | 'low';
			reason: string;
		}>;
		total: number;
	}> {
		return this.request('/recommendations/queue');
	}

	async submitRecommendationFeedback(bookId: string, feedback: 'dismiss' | 'not_interested' | 'like' = 'dismiss'): Promise<{ status: string; feedback: string }> {
		return this.request('/recommendations/feedback', {
			method: 'POST',
			body: JSON.stringify({ book_id: bookId, feedback }),
		});
	}

	async clearRecommendationFeedback(bookId: string): Promise<{ status: string }> {
		return this.request(`/recommendations/feedback/${bookId}`, { method: 'DELETE' });
	}

	// ─── Bookmarks ──────────────────────────────────────────────
	async getBookmarks(bookId: string): Promise<Bookmark[]> {
		return this.request(`/books/${bookId}/bookmarks`);
	}

	async createBookmark(bookId: string, data: {
		title?: string | null;
		name?: string;
		chapter_index?: number | null;
		position?: number | null;
		scroll_position?: number;
	}): Promise<Bookmark> {
		return this.request(`/books/${bookId}/bookmarks`, {
			method: 'POST',
			body: JSON.stringify({
				title: data.title ?? data.name ?? null,
				chapter_index: data.chapter_index ?? null,
				position: data.position ?? data.scroll_position ?? null,
			}),
		});
	}

	async deleteBookmark(bookId: string, bookmarkId: string): Promise<void> {
		await this.request(`/books/${bookId}/bookmarks/${bookmarkId}`, { method: 'DELETE' });
	}

	// ─── Activity Feed ──────────────────────────────────────────
	async getRecentActivities(limit: number = 10): Promise<Array<{
		id: string;
		type: string;
		message: string;
		description?: string;
		book_title?: string;
		book_id?: string;
		created_at: string;
	}>> {
		const rows = await this.request<Array<{
			id: string;
			type: string;
			message?: string;
			description?: string;
			book_title?: string;
			book_id?: string;
			created_at: string;
		}>>(`/stats/activities?limit=${limit}`);
		return rows.map((activity) => ({
			...activity,
			message: activity.message ?? activity.description ?? '',
		}));
	}

	// ─── Notifications ──────────────────────────────────────────
	async getNotifications(opts?: {
		category?: string;
		unreadOnly?: boolean;
		limit?: number;
		offset?: number;
	}): Promise<{ items: AppNotification[]; total: number; unread: number }> {
		const params = new URLSearchParams();
		if (opts?.category && opts.category !== 'all') params.set('category', opts.category);
		if (opts?.unreadOnly) params.set('unread_only', 'true');
		if (opts?.limit != null) params.set('limit', String(opts.limit));
		if (opts?.offset != null) params.set('offset', String(opts.offset));
		const qs = params.toString();
		return this.request(`/notifications${qs ? `?${qs}` : ''}`);
	}

	async getUnreadNotificationCount(): Promise<number> {
		const res = await this.request<{ count: number }>('/notifications/unread-count');
		return res.count;
	}

	async markNotificationRead(id: string): Promise<void> {
		await this.request(`/notifications/${id}/read`, { method: 'POST' });
	}

	async markAllNotificationsRead(): Promise<void> {
		await this.request('/notifications/read-all', { method: 'POST' });
	}

	async deleteNotification(id: string): Promise<void> {
		await this.request(`/notifications/${id}`, { method: 'DELETE' });
	}

	async clearNotifications(readOnly = false): Promise<void> {
		await this.request(`/notifications${readOnly ? '?read_only=true' : ''}`, { method: 'DELETE' });
	}

	// ─── AI ─────────────────────────────────────────────────────
	async streamAiChat(
		messages: Array<{ role: string; content: string }>,
		onToken: (token: string) => void,
		options?: { book_id?: string; include_rag?: boolean; temperature?: number }
	): Promise<void> {
		const response = await fetch(`${BASE_URL}/ai/chat/stream`, {
			method: 'POST',
			credentials: 'include',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ messages, ...options }),
		});
		if (!response.ok) throw new Error(`HTTP ${response.status}`);

		const reader = response.body!.getReader();
		const decoder = new TextDecoder();

		while (true) {
			const { done, value } = await reader.read();
			if (done) break;
			const text = decoder.decode(value, { stream: true });
			// Parse SSE format
			for (const line of text.split('\n')) {
				if (line.startsWith('data: ')) {
					const data = line.slice(6);
					if (data === '[DONE]') return;
					onToken(data);
				}
			}
		}
	}

	async aiSummarize(text: string, style?: 'brief' | 'detailed' | 'bullet_points'): Promise<AiSummaryResult> {
		return this.request('/ai/summarize', {
			method: 'POST',
			body: JSON.stringify({ text, style }),
		});
	}

	async aiExtractEntities(text: string, bookId?: string): Promise<AiExtractEntitiesResult> {
		return this.request('/ai/extract-entities', {
			method: 'POST',
			body: JSON.stringify({ text, book_id: bookId }),
		});
	}

	async aiAnalyzeStyle(text: string): Promise<{
		tone: string;
		pov: string;
		avg_sentence_length: number;
		vocabulary_richness: number;
		dialogue_ratio: number;
		description_style: string;
		pacing: string;
		suggestions: string[];
	}> {
		return this.request('/ai/analyze-style', {
			method: 'POST',
			body: JSON.stringify({ text }),
		});
	}

	async aiSuggestTags(title: string, description?: string, contentSample?: string): Promise<AiSuggestedTagsResult> {
		return this.request('/ai/suggest-tags', {
			method: 'POST',
			body: JSON.stringify({ title, description, content_sample: contentSample }),
		});
	}

	async aiGenerateOutline(premise: string, genre?: string, chapterCount?: number): Promise<{
		title_suggestions: string[];
		chapters: Array<{ title: string; summary: string; key_events: string[] }>;
	}> {
		return this.request('/ai/generate-outline', {
			method: 'POST',
			body: JSON.stringify({ premise, genre, chapter_count: chapterCount }),
		});
	}

	/**
	 * Batch process an entire book with AI.
	 * Performs: summarize + extract entities + tag + style analyze + embeddings (optional)
	 * This is the "让 AI 一口气读完整本小说" feature.
	 */
	async aiBatchProcess(bookId: string, operations: string[] = ['summarize', 'entities', 'tags', 'style', 'embeddings']): Promise<{
		book_id: string;
		status: string;
		chapters_processed: number;
		entities_found: number;
		tags_generated: string[];
		style: Record<string, string> | null;
	}> {
		return this.request('/ai/batch-process', {
			method: 'POST',
			body: JSON.stringify({ book_id: bookId, operations }),
		});
	}

	/**
	 * Generate and index embeddings for a book's chapters into Qdrant.
	 */
	async aiIngestEmbeddings(bookId: string, chapterIndices?: number[]): Promise<{
		chunks_indexed: number;
		collection: string;
	}> {
		return this.request('/ai/ingest-embeddings', {
			method: 'POST',
			body: JSON.stringify({ book_id: bookId, chapter_indices: chapterIndices }),
		});
	}

	// ─── Duplicate Detection ────────────────────────────────────
	async startDuplicateScan(input: StartDuplicateScanInput = {}): Promise<DuplicateScan> {
		return this.request('/duplicates/scans', {
			method: 'POST',
			body: JSON.stringify(input),
		});
	}

	async getDuplicatePairs(filters: DuplicatePairFilters = {}): Promise<DuplicatePairPage> {
		const params = new URLSearchParams();
		if (filters.candidate_kind) params.set('candidate_kind', filters.candidate_kind);
		if (filters.relation) params.set('relation', filters.relation);
		if (filters.status) params.set('status', filters.status);
		if (filters.library_id) params.set('library_id', filters.library_id);
		if (filters.limit !== undefined) params.set('limit', String(filters.limit));
		if (filters.offset !== undefined) params.set('offset', String(filters.offset));
		const query = params.toString();
		return this.request(`/duplicates${query ? `?${query}` : ''}`);
	}

	async getExactFileDiscoveries(filters: {
		library_id?: string;
		limit?: number;
		offset?: number;
	} = {}): Promise<ExactFileDiscoveryPage> {
		const params = new URLSearchParams();
		if (filters.library_id) params.set('library_id', filters.library_id);
		if (filters.limit !== undefined) params.set('limit', String(filters.limit));
		if (filters.offset !== undefined) params.set('offset', String(filters.offset));
		const query = params.toString();
		return this.request(`/duplicates/exact-file-discoveries${query ? `?${query}` : ''}`);
	}

	async getLatestDuplicateScan(libraryId?: string): Promise<DuplicateScan | null> {
		try {
			const query = libraryId ? `?library_id=${encodeURIComponent(libraryId)}` : '';
			return await this.request(`/duplicates/scans/latest${query}`);
		} catch (error) {
			if (error instanceof ApiRequestError && error.isNotFound) return null;
			throw error;
		}
	}

	async getDuplicatePair(
		id: string,
		pagination: { match_limit?: number; match_offset?: number } = {},
	): Promise<DuplicatePairDetail> {
		const params = new URLSearchParams();
		if (pagination.match_limit !== undefined) params.set('match_limit', String(pagination.match_limit));
		if (pagination.match_offset !== undefined) params.set('match_offset', String(pagination.match_offset));
		const query = params.toString();
		return this.request(`/duplicates/${id}${query ? `?${query}` : ''}`);
	}

	async resolveDuplicatePair(
		id: string,
		input: { action: DuplicateResolutionAction },
	): Promise<DuplicateResolutionResult> {
		return this.request(`/duplicates/${id}/resolve`, {
			method: 'POST',
			body: JSON.stringify(input),
		});
	}

	async getDuplicateMatchDiff(pairId: string, matchId: string): Promise<DuplicateMatchDiff> {
		return this.request(`/duplicates/${pairId}/matches/${matchId}/diff`);
	}

	// ─── Export / Import ────────────────────────────────────────
	async exportAnnotations(bookId: string, format: 'markdown' | 'json' | 'notion'): Promise<Blob> {
		const response = await fetch(`${BASE_URL}/books/${bookId}/annotations/export?format=${format}`, {
			credentials: 'include',
		});
		if (!response.ok) throw new Error(`HTTP ${response.status}`);
		const body = await response.json();
		const data = body && typeof body === 'object' && 'code' in body && 'data' in body && 'timestamp' in body
			? body.data
			: body;

		if (format === 'markdown') {
			return new Blob([String(data?.content ?? '')], { type: 'text/markdown;charset=utf-8' });
		}

		const payload = format === 'notion'
			? { blocks: data?.blocks ?? [], format: 'notion' }
			: { annotations: data?.annotations ?? [], format: 'json' };
		return new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json;charset=utf-8' });
	}

	// ─── Settings ───────────────────────────────────────────────
	async getSettings(): Promise<AppSettings> {
		return this.request('/settings');
	}

	async updateSettings(settings: Partial<AppSettings>): Promise<void> {
		await this.request('/settings', {
			method: 'PUT',
			body: JSON.stringify(settings),
		});
	}

	// ─── Health / Admin ─────────────────────────────────────────
	async getHealth(): Promise<{
		status: string;
		database: boolean;
		redis: boolean;
		qdrant: boolean;
		meilisearch: boolean;
		version: string;
		uptime_seconds: number;
	}> {
		return this.request('/health');
	}

	async getSystemStats(): Promise<{
		total_books: number;
		total_annotations: number;
		total_entities: number;
		total_chapters: number;
		storage_used_bytes: number;
		tasks_pending: number;
		tasks_completed: number;
	}> {
		return this.request('/stats');
	}

	async getTaskQueue(params?: { status?: string; book_id?: string; category?: string; page?: number; per_page?: number }): Promise<{
		data: Array<{
			id: string;
			kind: string;
			status: string;
			priority: string;
			progress: number;
			progress_message: string | null;
			error_message: string | null;
			book_id: string | null;
			category: string;
			retry_count: number;
			max_retries: number;
			payload: Record<string, unknown>;
			result: Record<string, unknown> | null;
			created_at: string;
			started_at: string | null;
			completed_at: string | null;
		}>;
		total: number;
		page: number;
		per_page: number;
	}> {
		const query = new URLSearchParams();
		if (params) {
			Object.entries(params).forEach(([k, v]) => {
				if (v != null) query.set(k, String(v));
			});
		}
		return this.request(`/tasks?${query}`);
	}

	async getTaskStats(): Promise<{
		stats: { queued: number; running: number; completed_today: number; failed_today: number; dead_letter_count: number; avg_processing_time_ms: number };
		categories: Array<{ category: string; queued: number; running: number; completed_today: number }>;
	}> {
		return this.request('/tasks/stats');
	}

	async submitPipeline(bookId: string, pipeline: 'full' | 'reindex' | 'deep_analysis' = 'full'): Promise<{ message: string; task_ids: string[] }> {
		return this.request('/tasks/submit-pipeline', {
			method: 'POST',
			body: JSON.stringify({ book_id: bookId, pipeline }),
		});
	}

	// ─── Deep Analysis results ──────────────────────────────────────
	async getAnalysisOverview(bookId: string): Promise<{
		chapter_summaries: number;
		sentiment_arcs: number;
		foreshadowing_total: number;
		foreshadowing_unresolved: number;
		macro_windows: number;
		has_deep_analysis: boolean;
	}> {
		return this.request(`/analysis/${bookId}/overview`);
	}

	async getChapterSummaries(bookId: string, limit = 200): Promise<Array<{
		id: string;
		chapter_index: number;
		summary: string;
		time_marker?: string | null;
		location?: string | null;
		key_event?: string | null;
		sentiment?: string | null;
		sentiment_score?: number | null;
		characters_present: string[];
		potential_mysteries: string[];
	}>> {
		return this.request(`/analysis/${bookId}/summaries?limit=${limit}`);
	}

	async getSentimentArc(bookId: string): Promise<{
		data: Array<{ chapter: number; overall: number; dominant?: string | null; is_peak: boolean; is_valley: boolean }>;
		stats: { average_score: number; peaks: number[]; valleys: number[]; total_chapters: number };
	}> {
		return this.request(`/analysis/${bookId}/sentiment`);
	}

	async getForeshadowing(bookId: string, status?: string): Promise<{
		entries: Array<{
			id: string;
			setup_chapter: number;
			setup_description: string;
			payoff_chapter?: number | null;
			payoff_description?: string | null;
			confidence: number;
			status: string;
			category: string;
		}>;
		stats: { total: number; unresolved: number; resolved: number };
	}> {
		const qs = status ? `?status=${encodeURIComponent(status)}` : '';
		return this.request(`/analysis/${bookId}/foreshadowing${qs}`);
	}

	async getStateChanges(bookId: string, character?: string): Promise<Array<{
		id: string;
		character_name: string;
		chapter_index: number;
		state_type: string;
		from_state?: string | null;
		to_state: string;
		trigger_event?: string | null;
		significance: number;
	}>> {
		const qs = character ? `?character=${encodeURIComponent(character)}` : '';
		return this.request(`/analysis/${bookId}/state-changes${qs}`);
	}

	// ─── Admin: Users ───────────────────────────────────────────────

	async getUsers(): Promise<Array<{
		id: string;
		username: string;
		display_name: string | null;
		role: 'admin' | 'reader' | 'guest';
		created_at: string;
		last_login_at: string | null;
		books_count: number;
		reading_time_hours: number;
	}>> {
		return this.request('/admin/users');
	}

	async updateUser(userId: string, data: { display_name?: string | null; role?: 'admin' | 'reader' | 'guest' }): Promise<{
		id: string;
		username: string;
		display_name: string | null;
		role: 'admin' | 'reader' | 'guest';
		created_at: string;
		last_login_at: string | null;
		books_count: number;
		reading_time_hours: number;
	}> {
		return this.request(`/admin/users/${userId}`, {
			method: 'PATCH',
			body: JSON.stringify(data),
		});
	}

	async deleteUser(userId: string): Promise<void> {
		await this.request(`/admin/users/${userId}`, { method: 'DELETE' });
	}

	async batchUpdateUserRole(userIds: string[], role: 'admin' | 'reader' | 'guest'): Promise<{ updated: number }> {
		return this.request('/admin/users/batch-role', {
			method: 'POST',
			body: JSON.stringify({ user_ids: userIds, role }),
		});
	}

	// ─── Admin: User Groups ─────────────────────────────────────────

	async getGroups(): Promise<Array<{
		id: string;
		name: string;
		description: string;
		color: string;
		created_at: string;
		member_count: number;
		member_ids: string[];
	}>> {
		return this.request('/admin/groups');
	}

	async createGroup(data: { name: string; description?: string; color?: string }): Promise<{ id: string; name: string; description: string; color: string; member_count: number; member_ids: string[] }> {
		return this.request('/admin/groups', {
			method: 'POST',
			body: JSON.stringify(data),
		});
	}

	async updateGroup(id: string, data: { name?: string; description?: string; color?: string }): Promise<{ status: string }> {
		return this.request(`/admin/groups/${id}`, {
			method: 'PATCH',
			body: JSON.stringify(data),
		});
	}

	async deleteGroup(id: string): Promise<{ status: string }> {
		return this.request(`/admin/groups/${id}`, { method: 'DELETE' });
	}

	async setGroupMembers(id: string, userIds: string[]): Promise<{ status: string; member_count: number }> {
		return this.request(`/admin/groups/${id}/members`, {
			method: 'POST',
			body: JSON.stringify({ user_ids: userIds }),
		});
	}

	// ─── Admin: Permission Templates ───────────────────────────────

	async getPermissionTemplates(): Promise<PermissionTemplate[]> {
		return this.request('/admin/permission-templates');
	}

	async createPermissionTemplate(data: {
		name: string;
		description?: string | null;
		can_read: boolean;
		can_write: boolean;
		can_manage: boolean;
	}): Promise<PermissionTemplate> {
		return this.request('/admin/permission-templates', {
			method: 'POST',
			body: JSON.stringify(data),
		});
	}

	async updatePermissionTemplate(id: string, data: Partial<Pick<PermissionTemplate, 'name' | 'description' | 'can_read' | 'can_write' | 'can_manage'>>): Promise<{ status: string }> {
		return this.request(`/admin/permission-templates/${id}`, {
			method: 'PATCH',
			body: JSON.stringify(data),
		});
	}

	async deletePermissionTemplate(id: string): Promise<{ status: string }> {
		return this.request(`/admin/permission-templates/${id}`, { method: 'DELETE' });
	}

	// ─── Admin: System Logs ─────────────────────────────────────────

	async getSystemLogs(params?: { level?: string; limit?: number; offset?: number }): Promise<Array<{
		timestamp: string;
		level: 'info' | 'warn' | 'error' | 'debug';
		target: string;
		message: string;
		fields?: Record<string, string>;
	}>> {
		const searchParams = new URLSearchParams();
		if (params?.level) searchParams.set('level', params.level);
		if (params?.limit) searchParams.set('limit', String(params.limit));
		if (params?.offset) searchParams.set('offset', String(params.offset));
		return this.request(`/admin/logs?${searchParams}`);
	}

	// ─── Admin: Scheduled Jobs ──────────────────────────────────────

	async getScheduledJobs(): Promise<Array<{
		id: string;
		name: string;
		cron: string;
		last_run: string | null;
		next_run: string;
		status: 'active' | 'paused';
		last_duration_ms: number | null;
		logs?: Array<{ time: string; level: 'info' | 'warn' | 'error'; message: string }>;
	}>> {
		return this.request('/admin/jobs');
	}

	async toggleJob(jobId: string, enabled: boolean): Promise<void> {
		await this.request(`/admin/jobs/${jobId}`, {
			method: 'PATCH',
			body: JSON.stringify({ enabled }),
		});
	}

	// ─── Admin: Health Check & Orphans ──────────────────────────────

	async getBooksHealthCheck(): Promise<{
		total_issues: number;
		issues: {
			missing_cover: number;
			no_chapters: number;
			abnormal_progress: number;
			zero_word_count: number;
		};
		status: string;
	}> {
		return this.request('/admin/health-check');
	}

	async detectOrphanBooks(): Promise<{
		total_checked: number;
		orphans_found: number;
		orphans: Array<{ id: string; title: string; file_path: string; library_id: string | null }>;
	}> {
		return this.request('/admin/orphans');
	}

	async recalculateMetadata(): Promise<{
		word_count_updated: number;
		authors_linked: number;
		message: string;
	}> {
		return this.request('/admin/recalculate', { method: 'POST' });
	}

	// ─── AI Usage Tracking ──────────────────────────────────────────

	async getAiUsageSummary(days?: number): Promise<{
		request_count: number;
		total_prompt_tokens: number;
		total_completion_tokens: number;
		total_tokens: number;
		total_cost_cents: number;
		avg_latency_ms: number;
		error_count: number;
		error_rate: number;
	}> {
		return this.request(`/ai/usage/summary?days=${days ?? 30}`);
	}

	async getAiUsageDaily(days?: number): Promise<Array<{
		date: string;
		requests: number;
		tokens: number;
		cost_cents: number;
	}>> {
		return this.request(`/ai/usage/daily?days=${days ?? 30}`);
	}

	async getAiUsageOperations(days?: number): Promise<Array<{
		operation: string;
		count: number;
		tokens: number;
		cost_cents: number;
		avg_latency_ms: number;
	}>> {
		return this.request(`/ai/usage/operations?days=${days ?? 30}`);
	}

	async getAiUsageRecent(days?: number, limit = 50): Promise<Array<{
		id: string;
		operation: string;
		model: string;
		provider: string;
		total_tokens: number;
		cost_cents: number;
		latency_ms: number;
		request_summary: string | null;
		success: boolean;
		error_message: string | null;
		username: string | null;
		book_title: string | null;
		created_at: string;
	}>> {
		return this.request(`/ai/usage/recent?days=${days ?? 30}&limit=${limit}`);
	}

	// ─── Entity Profiles ────────────────────────────────────────────

	async getEntityProfile(entityId: string): Promise<{
		entity_id: string;
		name: string;
		entity_type: string;
		appearance: string | null;
		personality: string | null;
		background: string | null;
		abilities: string | null;
		motivation: string | null;
		arc_summary: string | null;
		attributes: Record<string, string>;
		timeline: Array<{ chapter: number; snippet: string }>;
		confidence_score: number;
		last_updated_by: string;
	}> {
		return this.request(`/entities/${entityId}/profile`);
	}

	async generateEntityProfile(entityId: string): Promise<{
		entity_id: string;
		name: string;
		entity_type: string;
		appearance: string | null;
		personality: string | null;
		background: string | null;
		abilities: string | null;
		motivation: string | null;
		arc_summary: string | null;
		attributes: Record<string, string>;
		timeline: Array<{ chapter: number; snippet: string }>;
		confidence_score: number;
		last_updated_by?: string;
	}> {
		return this.request(`/entities/${entityId}/profile/generate`, { method: 'POST' });
	}

	async getEntityTimeline(entityId: string): Promise<Array<{
		chapter_index: number;
		chapter_title: string | null;
		context: string | null;
		position?: number;
	}>> {
		const result = await this.request<unknown>(`/entities/${entityId}/timeline`);
		if (Array.isArray(result)) {
			return result as Array<{
				chapter_index: number;
				chapter_title: string | null;
				context: string | null;
				position?: number;
			}>;
		}
		if (result && typeof result === 'object' && 'timeline' in result) {
			const wrapped = result as {
				timeline?: Array<{
					chapter_index?: number | null;
					chapter_title?: string | null;
					context?: string | null;
					position?: number;
				}>;
			};
			return (wrapped.timeline ?? []).map((event) => ({
				chapter_index: event.chapter_index ?? 0,
				chapter_title: event.chapter_title ?? null,
				context: event.context ?? null,
				position: event.position,
			}));
		}
		return [];
	}

	// ─── Smart Shelves ───────────────────────────────────────────────────────

	async getSmartShelves(): Promise<SmartShelf[]> {
		return this.request('/shelves/smart');
	}

	async createSmartShelf(data: { name: string; description?: string; filter_criteria: Record<string, unknown> }): Promise<SmartShelf> {
		return this.request('/shelves/smart', { method: 'POST', body: JSON.stringify(data) });
	}

	async getSmartShelfBooks(shelfId: string): Promise<SmartShelfBook[]> {
		const result = await this.request<unknown>(`/shelves/smart/${shelfId}/books`);
		return this.unwrapArray<SmartShelfBook>(result);
	}

	async reorderShelf(shelfId: string, bookIds: string[]): Promise<{ status: string }> {
		return this.request(`/shelves/${shelfId}/reorder`, {
			method: 'PUT',
			body: JSON.stringify({ book_ids: bookIds }),
		});
	}

	// ─── Custom Metadata Fields ──────────────────────────────────────────────

	async getBookCustomFields(bookId: string): Promise<Record<string, unknown>> {
		return this.request(`/books/${bookId}/custom-fields`);
	}

	async updateBookCustomFields(bookId: string, fields: Record<string, unknown>): Promise<Record<string, unknown>> {
		return this.request(`/books/${bookId}/custom-fields`, {
			method: 'PUT',
			body: JSON.stringify(fields),
		});
	}

	// ─── Annotation Sharing ──────────────────────────────────────────────────

	async shareAnnotation(annotationId: string): Promise<{ token: string }> {
		return this.request(`/annotations/${annotationId}/share`, { method: 'POST' });
	}

	async getSharedAnnotation(token: string): Promise<{
		id?: string;
		book_id?: string;
		chapter_index?: number;
		start_offset?: number;
		end_offset?: number;
		selected_text?: string;
		text?: string;
		content?: string;
		note?: string | null;
		color?: string;
		book_title?: string;
		book_author?: string | null;
		chapter_title?: string;
	}> {
		return this.request(`/shared/annotations/${token}`);
	}

	// ─── AI Features ─────────────────────────────────────────────────────────

	async extractGlossary(
		pairs: Array<{ source: string; target: string }>,
		sourceLang = 'zh',
		targetLang = 'en'
	): Promise<GlossaryExtractionResult> {
		return this.request('/ai/extract-glossary', {
			method: 'POST',
			body: JSON.stringify({ pairs, source_lang: sourceLang, target_lang: targetLang }),
		});
	}

	async detectPlotHoles(bookId: string): Promise<PlotHoleReport> {
		return this.request('/ai/detect-plot-holes', {
			method: 'POST',
			body: JSON.stringify({ book_id: bookId }),
		});
	}

	async generateChapterTitles(bookId: string, chapterIndex: number): Promise<ChapterTitleResult> {
		return this.request('/ai/generate-chapter-titles', {
			method: 'POST',
			body: JSON.stringify({ book_id: bookId, chapter_index: chapterIndex }),
		});
	}

	async cleanupForumText(text: string): Promise<CleanupForumTextResult> {
		return this.request('/ai/cleanup-forum-text', {
			method: 'POST',
			body: JSON.stringify({ text }),
		});
	}

	// ─── Glossary Lookup ─────────────────────────────────────────────────────

	async lookupGlossaryTerm(term: string, bookId?: string): Promise<{ definition?: string }> {
		const query = new URLSearchParams({ term });
		if (bookId) query.set('book_id', bookId);
		const result = await this.request<unknown>(`/glossary/lookup?${query.toString()}`);
		if (result && typeof result === 'object' && 'data' in result) {
			const rows = (result as { data?: Array<{ definition?: string | null; target_term?: string | null }> }).data ?? [];
			const first = rows[0];
			return { definition: first?.definition ?? first?.target_term ?? undefined };
		}
		return result as { definition?: string };
	}

	// ─── Semantic Intelligence ───────────────────────────────────────────────

	async getSemanticProfiles(): Promise<SemanticProfile[]> {
		const result = await this.request<unknown>('/semantic-tags/profiles');
		return this.unwrapArray(result);
	}

	async createSemanticProfile(data: {
		name: string;
		description?: string | null;
		category: string;
		color: string;
		reference_texts: string[];
		match_threshold: number;
		is_warning: boolean;
	}): Promise<SemanticProfile> {
		return this.request('/semantic-tags/profiles', {
			method: 'POST',
			body: JSON.stringify(data),
		});
	}

	async deleteSemanticProfile(id: string): Promise<void> {
		return this.request(`/semantic-tags/profiles/${id}`, { method: 'DELETE' });
	}

	async computeProfileEmbedding(id: string): Promise<{ success: boolean }> {
		return this.request(`/semantic-tags/profiles/${id}/compute-embedding`, { method: 'POST' });
	}

	async getBookTagScores(bookId: string): Promise<TagScore[]> {
		const result = await this.request<unknown>(`/semantic-tags/books/${bookId}/scores`);
		return this.unwrapArray(result);
	}

	async getBookHeatmap(bookId: string): Promise<TagHeatmapEntry[]> {
		const result = await this.request<unknown>(`/semantic-tags/books/${bookId}/heatmap`);
		if (Array.isArray(result)) return result as TagHeatmapEntry[];
		if (result && typeof result === 'object' && 'scores' in result && 'profiles' in result) {
			const contract = result as {
				scores?: Array<{
					chapter_index: number;
					tag_profile_id?: string;
					score?: number;
					avg_score?: number;
					top_chunk_score?: number;
					max_score?: number;
					match_count?: number;
				}>;
				profiles?: Array<{ id: string; name: string; color: string }>;
			};
			const profiles = new Map((contract.profiles ?? []).map((profile) => [profile.id, profile]));
			return (contract.scores ?? [])
				.map((score) => {
					const profileId = score.tag_profile_id;
					if (!profileId) return null;
					const profile = profiles.get(profileId);
					return {
						chapter_index: score.chapter_index,
						tag_profile_id: profileId,
						name: profile?.name ?? '未命名标签',
						color: profile?.color ?? '#94a3b8',
						avg_score: score.avg_score ?? score.score ?? 0,
						max_score: score.max_score ?? score.top_chunk_score ?? score.score ?? 0,
						match_count: score.match_count ?? 1,
					};
				})
				.filter((entry): entry is TagHeatmapEntry => entry !== null);
		}
		return this.unwrapArray(result);
	}

	async getBookMarkers(bookId: string): Promise<TagMarker[]> {
		const result = await this.request<unknown>(`/semantic-tags/books/${bookId}/markers`);
		return this.unwrapArray<Partial<TagMarker> & {
			tag_profile_id?: string;
			profile_id?: string;
			similarity_score?: number;
			score?: number;
			content_snippet?: string;
			snippet?: string;
			char_offset?: number | null;
			offset?: number | null;
		}>(result).map((raw) => {
			const score = raw.similarity_score ?? raw.score ?? 0;
			const snippet = raw.content_snippet ?? raw.snippet ?? '';
			const offset = raw.char_offset ?? raw.offset ?? null;
			return {
				...raw,
				tag_profile_id: raw.tag_profile_id ?? raw.profile_id ?? '',
				profile_id: raw.tag_profile_id ?? raw.profile_id ?? '',
				chapter_index: raw.chapter_index ?? 0,
				similarity_score: score,
				score,
				content_snippet: raw.content_snippet ?? raw.snippet ?? '',
				snippet,
				char_offset: offset,
				offset,
			};
		});
	}

	async computeBookTags(bookId: string): Promise<{ success: boolean }> {
		return this.request(`/semantic-tags/books/${bookId}/compute`, { method: 'POST' });
	}

	async getSemanticOverview(): Promise<SemanticOverview> {
		return this.request('/semantic-tags/overview');
	}

	async getBookRadar(bookId: string): Promise<BookRadarResult> {
		return this.request(`/semantic-tags/radar/${bookId}`);
	}

	async vibeSearch(data: { text: string; limit?: number; threshold?: number }): Promise<{ results: VibeSearchResult[] }> {
		return this.request('/search/vibe', {
			method: 'POST',
			body: JSON.stringify(data),
		});
	}

	async saveVibeBookmark(data: {
		name?: string | null;
		source_text: string;
		source_book_id?: string | null;
		source_chapter_index?: number | null;
	}): Promise<VibeBookmark> {
		return this.request('/search/vibe/bookmark', {
			method: 'POST',
			body: JSON.stringify({
				name: data.name ?? null,
				source_text: data.source_text,
				source_book_id: data.source_book_id ?? null,
				source_chapter_index: data.source_chapter_index ?? null,
			}),
		});
	}

	async getVibeBookmarks(): Promise<VibeBookmark[]> {
		const result = await this.request<unknown>('/search/vibe/bookmarks');
		return this.unwrapArray(result);
	}
}

export const api = new ApiClient();
