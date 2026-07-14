/**
 * TanStack Query hooks for Nova Reader.
 * These provide automatic caching, refetching, and optimistic updates.
 */
import { createQuery, createMutation, createInfiniteQuery, useQueryClient } from '@tanstack/svelte-query';
import { api } from '$services/api';
import type { Book, Chapter, Entity, Collection, SearchResult, SearchMode } from '$types/models';
import { toast } from 'svelte-sonner';

// ─────────────────────────────────────────────────────────────
// QUERY KEYS — centralized for invalidation
// ─────────────────────────────────────────────────────────────
export const queryKeys = {
	books: {
		all: ['books'] as const,
		list: (params?: Record<string, unknown>) => ['books', 'list', params] as const,
		detail: (id: string) => ['books', 'detail', id] as const,
		chapters: (bookId: string) => ['books', bookId, 'chapters'] as const,
		annotations: (bookId: string) => ['books', bookId, 'annotations'] as const,
		similar: (bookId: string) => ['books', bookId, 'similar'] as const,
	},
	libraries: {
		all: ['libraries'] as const,
		detail: (id: string) => ['libraries', 'detail', id] as const,
		series: (libraryId: string) => ['libraries', libraryId, 'series'] as const,
	},
	series: {
		all: ['series'] as const,
		detail: (id: string) => ['series', 'detail', id] as const,
		books: (seriesId: string) => ['series', seriesId, 'books'] as const,
	},
	collections: {
		all: ['collections'] as const,
		detail: (id: string) => ['collections', 'detail', id] as const,
		books: (id: string) => ['collections', id, 'books'] as const,
	},
	entities: {
		all: ['entities'] as const,
		detail: (id: string) => ['entities', 'detail', id] as const,
		graph: (params?: Record<string, unknown>) => ['entities', 'graph', params] as const,
	},
	characters: {
		all: ['characters'] as const,
		detail: (id: string) => ['characters', 'detail', id] as const,
	},
	glossary: {
		all: ['glossary'] as const,
		byBook: (bookId: string) => ['glossary', 'book', bookId] as const,
		bySeries: (seriesId: string) => ['glossary', 'series', seriesId] as const,
	},
	tasks: {
		all: ['tasks'] as const,
		stats: ['tasks', 'stats'] as const,
	},
	dashboard: {
		stats: ['dashboard', 'stats'] as const,
	},
	search: (query: string, mode: string) => ['search', query, mode] as const,
} as const;

// ─────────────────────────────────────────────────────────────
// LIBRARY QUERIES
// ─────────────────────────────────────────────────────────────
export function useLibraries() {
	return createQuery(() => ({
		queryKey: queryKeys.libraries.all,
		queryFn: () => api.getLibraries(),
	}));
}

export function useLibrarySeries(libraryId: () => string) {
	return createQuery(() => ({
		queryKey: queryKeys.libraries.series(libraryId()),
		queryFn: () => api.getLibrarySeries(libraryId()),
		enabled: !!libraryId(),
	}));
}

export function useScanLibrary() {
	const client = useQueryClient();
	return createMutation(() => ({
		mutationFn: (libraryId: string) => api.scanLibrary(libraryId),
		onSuccess: () => {
			client.invalidateQueries({ queryKey: queryKeys.libraries.all });
			toast.success('扫描已触发');
		},
		onError: () => toast.error('扫描失败'),
	}));
}

// ─────────────────────────────────────────────────────────────
// BOOK QUERIES
// ─────────────────────────────────────────────────────────────
export function useBooks(params?: () => Parameters<typeof api.getBooks>[0]) {
	return createQuery(() => ({
		queryKey: queryKeys.books.list(params?.()),
		queryFn: () => api.getBooks(params?.()),
	}));
}

export function useBook(id: () => string) {
	return createQuery(() => ({
		queryKey: queryKeys.books.detail(id()),
		queryFn: () => api.getBook(id()),
		enabled: !!id(),
	}));
}

export function useBookChapters(bookId: () => string) {
	return createQuery(() => ({
		queryKey: queryKeys.books.chapters(bookId()),
		queryFn: () => api.getChapters(bookId()),
		enabled: !!bookId(),
	}));
}

export function useUpdateBook() {
	const client = useQueryClient();
	return createMutation(() => ({
		mutationFn: ({ id, data }: { id: string; data: Partial<Book> }) => api.updateBook(id, data),
		onSuccess: (_, { id }) => {
			client.invalidateQueries({ queryKey: queryKeys.books.detail(id) });
			client.invalidateQueries({ queryKey: queryKeys.books.all });
			toast.success('书籍已更新');
		},
	}));
}

export function useDeleteBook() {
	const client = useQueryClient();
	return createMutation(() => ({
		mutationFn: (id: string) => api.deleteBook(id),
		onSuccess: () => {
			client.invalidateQueries({ queryKey: queryKeys.books.all });
			toast.success('书籍已删除');
		},
	}));
}

// ─────────────────────────────────────────────────────────────
// COLLECTION QUERIES
// ─────────────────────────────────────────────────────────────
export function useCollections() {
	return createQuery(() => ({
		queryKey: queryKeys.collections.all,
		queryFn: () => api.getCollections(),
	}));
}

export function useCreateCollection() {
	const client = useQueryClient();
	return createMutation(() => ({
		mutationFn: (data: { name: string; description?: string }) => api.createCollection(data),
		onSuccess: () => {
			client.invalidateQueries({ queryKey: queryKeys.collections.all });
			toast.success('合集已创建');
		},
	}));
}

export function useDeleteCollection() {
	const client = useQueryClient();
	return createMutation(() => ({
		mutationFn: (id: string) => api.deleteCollection(id),
		onSuccess: () => {
			client.invalidateQueries({ queryKey: queryKeys.collections.all });
			toast.success('合集已删除');
		},
	}));
}

// ─────────────────────────────────────────────────────────────
// ENTITY / CHARACTER QUERIES
// ─────────────────────────────────────────────────────────────
export function useEntities(params?: () => Parameters<typeof api.getEntities>[0]) {
	return createQuery(() => ({
		queryKey: queryKeys.entities.all,
		queryFn: () => api.getEntities(params?.()),
	}));
}

export function useEntity(id: () => string) {
	return createQuery(() => ({
		queryKey: queryKeys.entities.detail(id()),
		queryFn: () => api.getEntity(id()),
		enabled: !!id(),
	}));
}

export function useEntityGraph(params?: () => Parameters<typeof api.getEntityGraph>[0]) {
	return createQuery(() => ({
		queryKey: queryKeys.entities.graph(params?.()),
		queryFn: () => api.getEntityGraph(params?.()),
	}));
}

// ─────────────────────────────────────────────────────────────
// SEARCH
// ─────────────────────────────────────────────────────────────
export function useSearch(query: () => string, mode: () => SearchMode) {
	return createQuery(() => ({
		queryKey: queryKeys.search(query(), mode()),
		queryFn: () => api.search({ query: query(), mode: mode(), limit: 20 }),
		enabled: !!query(),
	}));
}

// ─────────────────────────────────────────────────────────────
// DASHBOARD
// ─────────────────────────────────────────────────────────────
export function useDashboardStats() {
	return createQuery(() => ({
		queryKey: queryKeys.dashboard.stats,
		queryFn: () => api.getDashboardStats(),
		refetchInterval: 30_000, // Refresh every 30s
	}));
}

// ─────────────────────────────────────────────────────────────
// TASKS
// ─────────────────────────────────────────────────────────────
export function useTaskStats() {
	return createQuery(() => ({
		queryKey: queryKeys.tasks.stats,
		queryFn: () => api.getTaskStats(),
		refetchInterval: 5_000, // Refresh every 5s
	}));
}

export function useTaskQueue(params?: () => Record<string, unknown>) {
	return createQuery(() => ({
		queryKey: [...queryKeys.tasks.all, 'queue', params?.()],
		queryFn: () => api.getTaskQueue(params?.()),
		refetchInterval: 3_000,
	}));
}

// ─────────────────────────────────────────────────────────────
// TAGS
// ─────────────────────────────────────────────────────────────
export function useBookTags() {
	return createQuery(() => ({
		queryKey: ['tags'],
		queryFn: () => api.getBookTags(),
	}));
}

// ─────────────────────────────────────────────────────────────
// READING
// ─────────────────────────────────────────────────────────────
export function useReadingBooks(params: () => Parameters<typeof api.getBooks>[0]) {
	return createQuery(() => ({
		queryKey: queryKeys.books.list(params()),
		queryFn: () => api.getBooks(params()),
	}));
}

// ─────────────────────────────────────────────────────────────
// ACTIVITIES
// ─────────────────────────────────────────────────────────────
export function useRecentActivities(limit: () => number) {
	return createQuery(() => ({
		queryKey: ['activities', 'recent', limit()],
		queryFn: () => api.getRecentActivities(limit()),
	}));
}

// ─────────────────────────────────────────────────────────────
// NOTIFICATIONS
// ─────────────────────────────────────────────────────────────
export function useNotifications(opts: () => { category?: string; unreadOnly?: boolean; limit?: number }) {
	return createQuery(() => ({
		queryKey: ['notifications', 'list', opts()],
		queryFn: () => api.getNotifications(opts()),
		refetchInterval: 60_000,
	}));
}

export function useUnreadNotificationCount() {
	return createQuery(() => ({
		queryKey: ['notifications', 'unread-count'],
		queryFn: () => api.getUnreadNotificationCount(),
		refetchInterval: 60_000,
	}));
}

export function useMarkNotificationRead() {
	const qc = useQueryClient();
	return createMutation(() => ({
		mutationFn: (id: string) => api.markNotificationRead(id),
		onSuccess: () => {
			qc.invalidateQueries({ queryKey: ['notifications'] });
		},
	}));
}

export function useMarkAllNotificationsRead() {
	const qc = useQueryClient();
	return createMutation(() => ({
		mutationFn: () => api.markAllNotificationsRead(),
		onSuccess: () => {
			qc.invalidateQueries({ queryKey: ['notifications'] });
		},
	}));
}

export function useDeleteNotification() {
	const qc = useQueryClient();
	return createMutation(() => ({
		mutationFn: (id: string) => api.deleteNotification(id),
		onSuccess: () => {
			qc.invalidateQueries({ queryKey: ['notifications'] });
		},
	}));
}

export function useClearNotifications() {
	const qc = useQueryClient();
	return createMutation(() => ({
		mutationFn: (readOnly?: boolean) => api.clearNotifications(readOnly ?? false),
		onSuccess: () => {
			qc.invalidateQueries({ queryKey: ['notifications'] });
		},
	}));
}
