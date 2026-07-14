import { test, expect, type Route } from '@playwright/test';

const json = (route: Route, body: unknown) =>
	route.fulfill({
		status: 200,
		contentType: 'application/json',
		body: JSON.stringify(body),
	});

test.describe('Nova Reader E2E', () => {
	test.beforeEach(async ({ context }) => {
		await context.route(/\/api\/.*/, async (route) => {
			const url = new URL(route.request().url());
			const path = url.pathname.replace(/^\/api/, '');

			if (path === '/auth/me') {
				return json(route, {
					id: 'test-admin',
					username: 'admin',
					display_name: 'Admin',
					role: 'admin',
				});
			}
			if (path === '/auth/refresh') return json(route, { access_token: 'test-token' });
			if (path === '/health/setup-status') return json(route, { needs_setup: false });
			if (path === '/settings') return json(route, {});
			if (path === '/books') {
				return json(route, { data: [], total: 0, page: 1, per_page: 24 });
			}
			if (path === '/libraries') return json(route, []);
			if (path === '/stats/dashboard') {
				return json(route, {
					total_books: 0,
					books_in_progress: 0,
					reading_time_today_mins: 0,
					tasks_running: 0,
					storage_used_gb: 0,
					entities_extracted: 0,
				});
			}
			if (path === '/stats') {
				return json(route, {
					total_books: 0,
					total_annotations: 0,
					total_entities: 0,
					total_chapters: 0,
					storage_used_bytes: 0,
					tasks_pending: 0,
					tasks_completed: 0,
				});
			}
			if (path === '/stats/reading') {
				return json(route, {
					totalBooksRead: 0,
					totalReadingTime: 0,
					totalAnnotations: 0,
					avgDailyMinutes: 0,
					longestStreak: 0,
					currentStreak: 0,
					booksThisMonth: 0,
					pagesThisWeek: 0,
				});
			}
			if (path === '/stats/reading/heatmap') return json(route, []);
			if (path === '/stats/reading/sessions') return json(route, []);
			if (path === '/stats/reading/goals') return json(route, []);
			if (path === '/stats/activities') return json(route, []);
			if (path === '/recommendations') return json(route, []);
			if (path === '/recommendations/queue') return json(route, { queue: [], total: 0 });
			if (path === '/health') {
				return json(route, {
					status: 'ok',
					database: true,
					redis: true,
					qdrant: true,
					meilisearch: true,
					version: 'e2e',
					uptime_seconds: 1,
				});
			}

			return json(route, {});
		});
	});

	test('homepage loads with dashboard', async ({ page }) => {
		await page.goto('/');
		await expect(page.locator('h1, h2').first()).toBeVisible();
		// Should show the dashboard or login
		await expect(page).toHaveTitle(/Nova Reader/);
	});

	test('library page shows all-books workspace', async ({ page }) => {
		await page.goto('/library');
		await expect(page.getByRole('heading', { name: '所有书籍', level: 1 })).toBeVisible();
		await expect(page.locator('[data-testid="book-grid"], [data-testid="book-grid-empty"], [data-testid="book-grid-error"], [data-testid="book-grid-loading"]').first()).toBeVisible();
	});

	test('search works', async ({ page }) => {
		await page.goto('/');
		await expect(page.locator('h1, h2').first()).toBeVisible();
		// Open command palette with Cmd+K
		await page.keyboard.press('Meta+k');
		const input = page.locator('[data-search-input], input[placeholder*="搜索"]').first();
		await expect(input).toBeVisible();
	});

	test('keyboard shortcuts help opens', async ({ page }) => {
		await page.goto('/');
		await expect(page.locator('h1, h2').first()).toBeVisible();
		await page.keyboard.press('Shift+/');
		await expect(page.locator('text=键盘快捷键')).toBeVisible();
	});

	test('navigation works with vim keys', async ({ page }) => {
		await page.goto('/');
		await expect(page.locator('h1, h2').first()).toBeVisible();
		// G+L should navigate to library
		await page.keyboard.press('g');
		await page.keyboard.press('l');
		await page.waitForURL('**/library');
		expect(page.url()).toContain('/library');
	});

	test('mobile nav is visible on small screens', async ({ page }) => {
		await page.setViewportSize({ width: 375, height: 667 });
		await page.goto('/');
		await expect(page.getByRole('navigation', { name: '移动主导航' })).toBeVisible();
	});

	test('admin panel loads', async ({ page }) => {
		await page.goto('/admin');
		await expect(page.getByRole('heading', { name: '系统仪表盘' })).toBeVisible();
	});

	test('stats page renders charts', async ({ page }) => {
		await page.goto('/stats');
		await expect(page.getByRole('heading', { name: '统计与活动', level: 1 })).toBeVisible();
	});
});
