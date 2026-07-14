import type { Book, Task, ReadingStats } from '$types/models';
import { api } from '$services/api';
import { toast } from 'svelte-sonner';

// Global app state using Svelte 5 runes
// Instantiated in +layout.svelte, consumed via context

class AppStore {
	// Dashboard stats
	dashboardStats = $state({
		total_books: 0,
		books_in_progress: 0,
		reading_time_today_mins: 0,
		tasks_running: 0,
		storage_used_gb: 0,
		entities_extracted: 0,
	});

	// Active tasks (updated via SSE)
	activeTasks = $state<Task[]>([]);

	// Task stream unsubscribe function
	private taskStreamCleanup: (() => void) | null = null;

	// Recent books for dashboard
	recentBooks = $state.raw<Book[]>([]);

	// Reading stats
	readingStats = $state<ReadingStats | null>(null);

	// Global loading state
	initializing = $state(true);

	async init() {
		try {
			await Promise.allSettled([
				this.refreshDashboard(),
				this.refreshActiveTasks(),
			]);
			this.connectTaskStream();
		} finally {
			this.initializing = false;
		}
	}

	async refreshDashboard() {
		try {
			this.dashboardStats = await api.getDashboardStats();
		} catch {
			// Silently fail on dashboard - non-critical
		}
	}

	async refreshActiveTasks() {
		try {
			this.activeTasks = await api.getTasks({ status: 'running', limit: 20 });
		} catch {
			// Silently fail
		}
	}

	connectTaskStream() {
		this.taskStreamCleanup = api.streamTasks((task) => {
			// Update or add task in active list
			const idx = this.activeTasks.findIndex(t => t.id === task.id);
			if (idx >= 0) {
				if (task.status === 'completed' || task.status === 'failed' || task.status === 'cancelled') {
					// Notify completion/failure
					if (task.status === 'completed') {
						toast.success(`任务完成: ${task.kind ?? ''}`, { duration: 4000 });
					} else if (task.status === 'failed') {
						toast.error(`任务失败: ${task.error ?? task.kind ?? ''}`, { duration: 6000 });
					}
					this.activeTasks = this.activeTasks.filter(t => t.id !== task.id);
				} else {
					this.activeTasks[idx] = task;
				}
			} else if (task.status === 'running' || task.status === 'queued') {
				this.activeTasks = [...this.activeTasks, task];
			}

			// Update dashboard running count
			this.dashboardStats.tasks_running = this.activeTasks.filter(t => t.status === 'running').length;
		});
	}

	destroy() {
		this.taskStreamCleanup?.();
	}
}

export const appStore = new AppStore();
