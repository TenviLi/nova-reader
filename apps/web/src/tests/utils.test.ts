import { describe, it, expect } from 'vitest';

/**
 * Tests for utility functions used across the Nova Reader frontend.
 */

describe('formatBytes', () => {
	function formatBytes(bytes: number): string {
		if (bytes === 0) return '0 B';
		const units = ['B', 'KB', 'MB', 'GB', 'TB'];
		const i = Math.floor(Math.log(bytes) / Math.log(1024));
		return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`;
	}

	it('should format 0 bytes', () => {
		expect(formatBytes(0)).toBe('0 B');
	});

	it('should format bytes', () => {
		expect(formatBytes(512)).toBe('512.0 B');
	});

	it('should format kilobytes', () => {
		expect(formatBytes(1024)).toBe('1.0 KB');
		expect(formatBytes(1536)).toBe('1.5 KB');
	});

	it('should format megabytes', () => {
		expect(formatBytes(1024 * 1024)).toBe('1.0 MB');
		expect(formatBytes(52428800)).toBe('50.0 MB');
	});

	it('should format gigabytes', () => {
		expect(formatBytes(1073741824)).toBe('1.0 GB');
	});
});

describe('URL construction', () => {
	it('should build query params correctly', () => {
		const params: Record<string, unknown> = {
			page: 2,
			per_page: 10,
			status: 'ready',
			language: null,
			search: undefined,
		};
		const query = new URLSearchParams();
		Object.entries(params).forEach(([k, v]) => {
			if (v != null) query.set(k, String(v));
		});

		expect(query.get('page')).toBe('2');
		expect(query.get('per_page')).toBe('10');
		expect(query.get('status')).toBe('ready');
		expect(query.has('language')).toBe(false);
		expect(query.has('search')).toBe(false);
	});
});

describe('timeAgo', () => {
	function timeAgo(dateStr: string): string {
		const now = Date.now();
		const date = new Date(dateStr).getTime();
		const seconds = Math.floor((now - date) / 1000);

		if (seconds < 60) return '刚刚';
		if (seconds < 3600) return `${Math.floor(seconds / 60)} 分钟前`;
		if (seconds < 86400) return `${Math.floor(seconds / 3600)} 小时前`;
		if (seconds < 604800) return `${Math.floor(seconds / 86400)} 天前`;
		return new Date(dateStr).toLocaleDateString('zh-CN');
	}

	it('should return "刚刚" for recent times', () => {
		const now = new Date().toISOString();
		expect(timeAgo(now)).toBe('刚刚');
	});

	it('should format minutes ago', () => {
		const fiveMinAgo = new Date(Date.now() - 5 * 60 * 1000).toISOString();
		expect(timeAgo(fiveMinAgo)).toBe('5 分钟前');
	});

	it('should format hours ago', () => {
		const twoHoursAgo = new Date(Date.now() - 2 * 60 * 60 * 1000).toISOString();
		expect(timeAgo(twoHoursAgo)).toBe('2 小时前');
	});

	it('should format days ago', () => {
		const threeDaysAgo = new Date(Date.now() - 3 * 24 * 60 * 60 * 1000).toISOString();
		expect(timeAgo(threeDaysAgo)).toBe('3 天前');
	});
});

describe('NAS exclude pattern matching (frontend side)', () => {
	// Mirror of the glob matching logic for client-side validation
	function matchesGlob(name: string, pattern: string): boolean {
		if (name.toLowerCase() === pattern.toLowerCase()) return true;
		// Simple glob with *
		const regex = pattern
			.replace(/[.+^${}()|[\]\\]/g, '\\$&')
			.replace(/\*/g, '.*')
			.replace(/\?/g, '.');
		return new RegExp(`^${regex}$`, 'i').test(name);
	}

	it('should match exact NAS patterns', () => {
		expect(matchesGlob('#recycle', '#recycle')).toBe(true);
		expect(matchesGlob('@eaDir', '@eaDir')).toBe(true);
		expect(matchesGlob('$RECYCLE.BIN', '$RECYCLE.BIN')).toBe(true);
	});

	it('should match wildcard patterns', () => {
		expect(matchesGlob('file.tmp', '*.tmp')).toBe(true);
		expect(matchesGlob('download.part', '*.part')).toBe(true);
		expect(matchesGlob('file.txt', '*.tmp')).toBe(false);
	});

	it('should match prefix wildcards', () => {
		expect(matchesGlob('~$document.docx', '~$*')).toBe(true);
		expect(matchesGlob('~$hello.xlsx', '~$*')).toBe(true);
		expect(matchesGlob('normal.docx', '~$*')).toBe(false);
	});

	it('should be case-insensitive', () => {
		expect(matchesGlob('THUMBS.DB', 'Thumbs.db')).toBe(true);
		expect(matchesGlob('thumbs.DB', 'Thumbs.db')).toBe(true);
	});
});
