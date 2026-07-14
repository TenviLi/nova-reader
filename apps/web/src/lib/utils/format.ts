/**
 * Format a number of seconds into a human-readable duration (Chinese)
 */
export function formatDuration(seconds: number): string {
	if (!Number.isFinite(seconds) || seconds < 0) return '0秒';
	if (seconds < 60) return `${Math.round(seconds)}秒`;
	if (seconds < 3600) return `${Math.floor(seconds / 60)}分钟`;
	const hours = Math.floor(seconds / 3600);
	const mins = Math.floor((seconds % 3600) / 60);
	return mins > 0 ? `${hours}小时${mins}分` : `${hours}小时`;
}

/**
 * Format a word count (Chinese convention: 万字)
 */
export function formatWordCount(count: number): string {
	if (count < 1000) return `${count}字`;
	if (count < 10000) return `${(count / 1000).toFixed(1)}千字`;
	return `${(count / 10000).toFixed(1)}万字`;
}

/**
 * Format a relative time string from ISO timestamp
 */
export function timeAgo(isoString: string): string {
	if (!isoString) return '';
	const now = Date.now();
	const then = new Date(isoString).getTime();
	if (Number.isNaN(then)) return '';
	const diff = Math.floor((now - then) / 1000);

	if (diff < 60) return '刚刚';
	if (diff < 3600) return `${Math.floor(diff / 60)}分钟前`;
	if (diff < 86400) return `${Math.floor(diff / 3600)}小时前`;
	if (diff < 604800) return `${Math.floor(diff / 86400)}天前`;
	return new Date(isoString).toLocaleDateString('zh-CN');
}

/**
 * Format file size in human-readable format
 */
export function formatFileSize(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
	if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MB`;
	return `${(bytes / 1073741824).toFixed(2)} GB`;
}

/**
 * Truncate text at a word boundary
 */
export function truncate(text: string, maxLength: number): string {
	if (text.length <= maxLength) return text;
	return text.slice(0, maxLength).trimEnd() + '…';
}

// Re-export debounce from es-toolkit for backward compatibility
export { debounce } from 'es-toolkit';
