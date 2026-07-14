import type { Annotation, Book, Chapter } from '$types/models';

/**
 * Export annotations as Markdown for use in note-taking apps.
 */
export function exportAnnotationsAsMarkdown(
	book: Book,
	annotations: Annotation[],
	chapters: Chapter[],
): string {
	const lines: string[] = [];
	lines.push(`# ${book.title} - 读书笔记`);
	lines.push('');
	lines.push(`**作者**: ${book.author}`);
	lines.push(`**导出时间**: ${new Date().toLocaleDateString('zh-CN')}`);
	lines.push(`**标注数量**: ${annotations.length}`);
	lines.push('');
	lines.push('---');
	lines.push('');

	// Group by chapter
	const byChapter = new Map<number, Annotation[]>();
	for (const ann of annotations) {
		const list = byChapter.get(ann.chapter_index) ?? [];
		list.push(ann);
		byChapter.set(ann.chapter_index, list);
	}

	for (const [chapterIdx, chapterAnns] of [...byChapter].sort((a, b) => a[0] - b[0])) {
		const chapter = chapters[chapterIdx];
		lines.push(`## ${chapter?.title ?? `第 ${chapterIdx + 1} 章`}`);
		lines.push('');

		for (const ann of chapterAnns.sort((a, b) => a.start_offset - b.start_offset)) {
			lines.push(`> ${ann.selected_text}`);
			lines.push('');
			if (ann.note) {
				lines.push(`💡 ${ann.note}`);
				lines.push('');
			}
			lines.push('---');
			lines.push('');
		}
	}

	return lines.join('\n');
}

/**
 * Export annotations as JSON for programmatic use.
 */
export function exportAnnotationsAsJson(book: Book, annotations: Annotation[]): string {
	return JSON.stringify({
		book: { id: book.id, title: book.title, author: book.author },
		exported_at: new Date().toISOString(),
		annotations: annotations.map(a => ({
			chapter_index: a.chapter_index,
			selected_text: a.selected_text,
			note: a.note,
			color: a.color,
			start_offset: a.start_offset,
			end_offset: a.end_offset,
		})),
	}, null, 2);
}

/**
 * Trigger a download in the browser.
 */
export function downloadFile(content: string, filename: string, mimeType = 'text/plain') {
	const blob = new Blob([content], { type: `${mimeType};charset=utf-8` });
	const url = URL.createObjectURL(blob);
	const link = document.createElement('a');
	link.href = url;
	link.download = filename;
	link.click();
	URL.revokeObjectURL(url);
}
