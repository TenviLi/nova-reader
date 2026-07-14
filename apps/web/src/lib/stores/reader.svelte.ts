import type { Book, Chapter, ReadingProgress, Annotation, Bookmark } from '$types/models';
import { api } from '$services/api';
import { debounce } from '$utils/format';

class ReaderStore {
	book = $state<Book | null>(null);
	chapters = $state.raw<Chapter[]>([]);
	currentChapterIndex = $state(0);
	content = $state('');
	entities = $state.raw<Array<{ start: number; end: number; type: string; name: string; id: string }>>([]);
	annotations = $state.raw<Annotation[]>([]);
	bookmarks = $state.raw<Bookmark[]>([]);
	loading = $state(false);
	scrollPosition = $state(0);

	// Sync conflict state
	syncConflict = $state<{
		server_progress: ReadingProgress;
		client_progress: ReadingProgress;
	} | null>(null);

	// Reading session
	private sessionStartTime: number | null = null;
	// Guard against race conditions: only the latest request's response is accepted
	private loadRequestId = 0;
	// Last known server timestamp for conflict detection
	private lastReadAt: string | null = null;

	currentChapter = $derived(this.chapters[this.currentChapterIndex] ?? null);
	progress = $derived(
		this.chapters.length > 0
			? (this.currentChapterIndex + this.scrollPosition) / this.chapters.length
			: 0
	);

	async loadBook(bookId: string) {
		this.loading = true;
		try {
			const [book, chapters, annotations, bookmarks] = await Promise.all([
				api.getBook(bookId),
				api.getChapters(bookId),
				api.getAnnotations(bookId).catch(() => []),
				api.getBookmarks(bookId).catch(() => []),
			]);
			this.book = book;
			this.chapters = chapters ?? [];
			this.annotations = annotations ?? [];
			this.bookmarks = bookmarks ?? [];

			// If no chapters exist yet, stop here (book not parsed)
			if (this.chapters.length === 0) {
				this.content = '';
				this.loading = false;
				return;
			}

			// Resume from last position (gracefully handle no prior progress)
			try {
				const progress = await api.getReadingProgress(bookId);
				this.currentChapterIndex = progress?.chapter_index ?? progress?.current_chapter ?? 0;
				this.scrollPosition = progress?.scroll_position ?? 0;
				this.lastReadAt = progress?.last_read_at ?? null;
			} catch {
				this.currentChapterIndex = 0;
				this.scrollPosition = 0;
				this.lastReadAt = null;
			}

			await this.loadChapter(this.currentChapterIndex);
			this.startSession();
		} finally {
			this.loading = false;
		}
	}

	async loadChapter(index: number) {
		if (!this.book) return;
		const requestId = ++this.loadRequestId;
		this.loading = true;
		try {
			const existingChapter = this.chapters[index];
			const chapterMetaPromise: Promise<Chapter | null> = existingChapter?.id
				? Promise.resolve(null)
				: api.getChapter(this.book.id, index).catch(() => null);
			const [result, chapterMeta] = await Promise.all([
				api.getChapterContent(this.book.id, index),
				chapterMetaPromise,
			]);
			// Stale response guard: discard if a newer request was issued
			if (requestId !== this.loadRequestId) return;
			if (chapterMeta) {
				this.chapters = this.chapters.map((chapter, chapterIndex) =>
					chapterIndex === index ? { ...chapter, ...chapterMeta } : chapter
				);
			}
			// Handle different response formats: { content, entities } or plain string
			if (typeof result === 'string') {
				this.content = result;
				this.entities = [];
			} else {
				this.content = result?.content ?? '';
				this.entities = result?.entities ?? [];
			}
			this.currentChapterIndex = index;
			this.scrollPosition = 0;
			this.saveProgress();
		} catch (e) {
			if (requestId === this.loadRequestId) {
				this.content = '';
				this.entities = [];
			}
		} finally {
			if (requestId === this.loadRequestId) {
				this.loading = false;
			}
		}
	}

	async nextChapter() {
		if (this.currentChapterIndex < this.chapters.length - 1) {
			await this.loadChapter(this.currentChapterIndex + 1);
		}
	}

	async prevChapter() {
		if (this.currentChapterIndex > 0) {
			await this.loadChapter(this.currentChapterIndex - 1);
		}
	}

	updateScroll(position: number) {
		this.scrollPosition = position;
		this.debouncedSaveProgress();
	}

	private debouncedSaveProgress = debounce(() => this.saveProgress(), 2000);

	private async saveProgress() {
		if (!this.book) return;
		try {
			const res = await api.updateReadingProgress(this.book.id, {
				chapter_index: this.currentChapterIndex,
				scroll_position: this.scrollPosition,
				progress: this.progress,
				client_last_read_at: this.lastReadAt ?? undefined,
			});
			if (res.status === 'conflict' && res.server_progress && res.client_progress) {
				// Multi-device conflict detected
				this.syncConflict = {
					server_progress: res.server_progress,
					client_progress: res.client_progress,
				};
			} else {
				// Update our last known timestamp
				this.lastReadAt = new Date().toISOString();
				this.syncConflict = null;
			}
		} catch {
			// Silent fail — will retry on next scroll
		}
	}

	/** Resolve sync conflict by choosing server or client progress */
	async resolveConflict(choice: 'server' | 'client') {
		if (!this.book || !this.syncConflict) return;
		if (choice === 'server') {
			// Accept server's progress
			const sp = this.syncConflict.server_progress;
			this.currentChapterIndex = sp.chapter_index ?? sp.current_chapter ?? 0;
			this.scrollPosition = 0;
			this.lastReadAt = sp.last_read_at ?? null;
			await this.loadChapter(this.currentChapterIndex);
		} else {
			// Force overwrite with our local progress
			await api.updateReadingProgress(this.book.id, {
				chapter_index: this.currentChapterIndex,
				scroll_position: this.scrollPosition,
				progress: this.progress,
				force: true,
			});
			this.lastReadAt = new Date().toISOString();
		}
		this.syncConflict = null;
	}

	private startSession() {
		this.sessionStartTime = Date.now();
	}

	async endSession() {
		if (!this.book || !this.sessionStartTime) return;
		await this.saveProgress();
		this.sessionStartTime = null;
	}

	async addAnnotation(data: {
		chapter_index: number;
		start_offset: number;
		end_offset: number;
		selected_text: string;
		note: string | null;
		color: string;
	}) {
		if (!this.book) return;
		const annotation = await api.createAnnotation(this.book.id, {
			...data,
			book_id: this.book.id,
		});
		this.annotations = [...this.annotations, annotation];
	}

	async deleteAnnotation(annotationId: string) {
		if (!this.book) return;
		await api.deleteAnnotation(this.book.id, annotationId);
		this.annotations = this.annotations.filter(a => a.id !== annotationId);
	}

	async addBookmark(data: {
		title?: string | null;
		chapter_index?: number | null;
		position?: number | null;
		scroll_position?: number;
	}) {
		if (!this.book) return null;
		const bookmark = await api.createBookmark(this.book.id, data);
		this.bookmarks = [bookmark, ...this.bookmarks.filter((item) => item.id !== bookmark.id)];
		return bookmark;
	}

	async deleteBookmark(bookmarkId: string) {
		if (!this.book) return;
		await api.deleteBookmark(this.book.id, bookmarkId);
		this.bookmarks = this.bookmarks.filter((bookmark) => bookmark.id !== bookmarkId);
	}

	cleanup() {
		this.endSession();
		this.book = null;
		this.chapters = [];
		this.content = '';
		this.entities = [];
		this.annotations = [];
		this.bookmarks = [];
	}
}

export const readerStore = new ReaderStore();
