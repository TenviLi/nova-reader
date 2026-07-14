<script lang="ts">
	interface Props {
		content: string;
		class?: string;
	}

	let { content, class: className = '' }: Props = $props();

	// Parse markdown to HTML (lightweight inline renderer)
	let html = $derived(renderMarkdown(content));

	function renderMarkdown(text: string): string {
		let result = escapeHtml(text);

		// Code blocks (```lang\n...\n```)
		result = result.replace(/```(\w*)\n([\s\S]*?)```/g, (_match, lang, code) => {
			const langLabel = lang ? `<span class="md-code-lang">${lang}</span>` : '';
			return `<div class="md-code-block">${langLabel}<pre><code>${code.trim()}</code></pre></div>`;
		});

		// Inline code
		result = result.replace(/`([^`]+)`/g, '<code class="md-inline-code">$1</code>');

		// Tables (simple)
		result = result.replace(
			/(?:^|\n)(\|.+\|)\n(\|[-| :]+\|)\n((?:\|.+\|\n?)*)/g,
			(_match, header, _separator, body) => {
				const headers = parsePipeRow(header);
				const rows = body.trim().split('\n').map(parsePipeRow);
				let table = '<div class="md-table-wrap"><table class="md-table"><thead><tr>';
				for (const h of headers) table += `<th>${h}</th>`;
				table += '</tr></thead><tbody>';
				for (const row of rows) {
					table += '<tr>';
					for (const cell of row) table += `<td>${cell}</td>`;
					table += '</tr>';
				}
				table += '</tbody></table></div>';
				return table;
			}
		);

		// Bold
		result = result.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
		// Italic
		result = result.replace(/\*(.+?)\*/g, '<em>$1</em>');
		// Strikethrough
		result = result.replace(/~~(.+?)~~/g, '<del>$1</del>');

		// Headings (### only for chat)
		result = result.replace(/^### (.+)$/gm, '<h4 class="md-h4">$1</h4>');
		result = result.replace(/^## (.+)$/gm, '<h3 class="md-h3">$1</h3>');

		// Unordered lists
		result = result.replace(/^[•\-\*] (.+)$/gm, '<li class="md-li">$1</li>');
		result = result.replace(/(<li class="md-li">.*<\/li>\n?)+/g, (m) => `<ul class="md-ul">${m}</ul>`);

		// Newlines to <br> (but not inside code blocks)
		result = result.replace(/\n/g, '<br>');

		return result;
	}

	function escapeHtml(text: string): string {
		return text
			.replace(/&/g, '&amp;')
			.replace(/</g, '&lt;')
			.replace(/>/g, '&gt;');
	}

	function parsePipeRow(row: string): string[] {
		return row
			.split('|')
			.slice(1, -1)
			.map(cell => cell.trim());
	}
</script>

<div class="md-content {className}">
	{@html html}
</div>

<style>
	.md-content :global(.md-code-block) {
		margin: 0.5rem 0;
		border-radius: 0.5rem;
		background: rgba(0, 0, 0, 0.3);
		border: 1px solid rgba(255, 255, 255, 0.06);
		overflow: hidden;
	}
	.md-content :global(.md-code-lang) {
		display: block;
		padding: 0.25rem 0.75rem;
		font-size: 0.6rem;
		color: rgba(255, 255, 255, 0.4);
		border-bottom: 1px solid rgba(255, 255, 255, 0.06);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	.md-content :global(pre) {
		padding: 0.75rem;
		overflow-x: auto;
		font-size: 0.8rem;
		line-height: 1.5;
		font-family: var(--font-mono, ui-monospace, monospace);
	}
	.md-content :global(.md-inline-code) {
		padding: 0.1em 0.35em;
		border-radius: 0.25rem;
		background: rgba(255, 255, 255, 0.06);
		font-size: 0.85em;
		font-family: var(--font-mono, ui-monospace, monospace);
	}
	.md-content :global(.md-table-wrap) {
		margin: 0.5rem 0;
		overflow-x: auto;
		border-radius: 0.5rem;
		border: 1px solid rgba(255, 255, 255, 0.06);
	}
	.md-content :global(.md-table) {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.8rem;
	}
	.md-content :global(.md-table th) {
		padding: 0.4rem 0.6rem;
		text-align: left;
		font-weight: 600;
		background: rgba(255, 255, 255, 0.04);
		border-bottom: 1px solid rgba(255, 255, 255, 0.08);
	}
	.md-content :global(.md-table td) {
		padding: 0.4rem 0.6rem;
		border-bottom: 1px solid rgba(255, 255, 255, 0.04);
	}
	.md-content :global(.md-h3) {
		font-size: 0.9rem;
		font-weight: 700;
		margin: 0.5rem 0 0.25rem;
	}
	.md-content :global(.md-h4) {
		font-size: 0.85rem;
		font-weight: 600;
		margin: 0.5rem 0 0.25rem;
	}
	.md-content :global(.md-ul) {
		list-style: none;
		padding-left: 0;
		margin: 0.25rem 0;
	}
	.md-content :global(.md-li) {
		position: relative;
		padding-left: 1rem;
	}
	.md-content :global(.md-li::before) {
		content: '•';
		position: absolute;
		left: 0;
		color: rgba(255, 255, 255, 0.3);
	}
	.md-content :global(strong) {
		font-weight: 600;
	}
</style>
