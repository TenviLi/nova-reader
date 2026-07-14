import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const appRoot = process.cwd();

function readRoute(path: string): string {
	return readFileSync(resolve(appRoot, path), 'utf8');
}

describe('product information architecture routes', () => {
	it('/library remains the all-books workspace without library overview cards', () => {
		const libraryPage = readRoute('src/routes/library/+page.svelte');

		expect(libraryPage).toContain('<BookGrid');
		expect(libraryPage).not.toContain('Library overview cards');
		expect(libraryPage).not.toContain('href="/libraries/{lib.id}"');
		expect(libraryPage).not.toContain('api.createLibrary');
		expect(libraryPage).not.toContain('api.uploadBook');
		expect(libraryPage).not.toContain('ondrop={handleDrop}');
		expect(libraryPage).not.toContain('拖放文件到此处上传');
	});

	it('/libraries exists as the library management overview', () => {
		const librariesPagePath = resolve(appRoot, 'src/routes/libraries/+page.svelte');

		expect(existsSync(librariesPagePath)).toBe(true);
		const librariesPage = readFileSync(librariesPagePath, 'utf8');
		expect(librariesPage).toContain('api.getLibraries');
		expect(librariesPage).toContain('LibraryDialog');
		expect(librariesPage).toContain('href="/libraries/{lib.id}"');
		expect(librariesPage).toContain('aria-label="筛选书库"');
		expect(librariesPage).toContain('name="library-management-search"');
		expect(librariesPage).toContain('placeholder="筛选书库…"');
	});

	it('library management actions are admin-aware and keep one settings destination', () => {
		const librariesPage = readRoute('src/routes/libraries/+page.svelte');
		const sidebar = readRoute('src/lib/components/layout/Sidebar.svelte');
		const cardLoop = librariesPage
			.split('{#each filteredLibraries() as lib}')
			.at(1) ?? '';

		expect(librariesPage).toContain("auth.user?.role === 'admin'");
		expect(librariesPage).toContain('{#if isAdmin}');
		expect(cardLoop).not.toContain('openEditDialog(lib)');
		expect(cardLoop).not.toContain('title="编辑"');
		expect(cardLoop).toContain('goto(`/libraries/${lib.id}/edit`)');
		expect(sidebar).toContain('{#if isAdmin}');
		expect(sidebar).not.toContain('添加书库');
	});

	it('library creation and management recovery links point to /libraries', () => {
		const homePage = readRoute('src/routes/+page.svelte');
		const libraryEditPage = readRoute('src/routes/libraries/[id]/edit/+page.svelte');
		const libraryDetailPage = readRoute('src/routes/libraries/[id]/+page.svelte');
		const libraryAnalyzePage = readRoute('src/routes/libraries/[id]/analyze/+page.svelte');
		const legacyManageLoad = readRoute('src/routes/library/manage/[id]/+page.ts');

		expect(homePage).toContain('href="/libraries"');
		expect(homePage).not.toContain('href="/library"\n');
		expect(libraryEditPage).toContain("goto('/libraries')");
		expect(libraryEditPage).not.toContain("goto('/library')");
		expect(libraryEditPage).toContain('href="/libraries"');
		expect(libraryDetailPage).toContain('href="/libraries"');
		expect(libraryAnalyzePage).toContain('href="/libraries"');
		expect(legacyManageLoad).toContain("redirect(308, `/libraries/${params.id}/edit`)");
	});

	it('library workspace series mosaic uses cover paths for cover URLs', () => {
		const libraryDetailPage = readRoute('src/routes/libraries/[id]/+page.svelte');

		expect(libraryDetailPage).toContain('normalizeCoverPath(book.cover_path)');
		expect(libraryDetailPage).not.toContain('/api/covers/{book.cover_path}');
		expect(libraryDetailPage).not.toContain('/api/covers/{book.id}');
	});

	it('book cards keep navigation and card actions as sibling controls', () => {
		const bookCard = readRoute('src/lib/components/library/BookCard.svelte');
		const contextMenu = readRoute('src/lib/components/library/BookContextMenu.svelte');

		expect(bookCard).toContain('class="absolute inset-0');
		expect(bookCard).toContain('aria-label={`打开 ${book.title}`}');
		expect(bookCard).toContain('class="pointer-events-none');
		expect(bookCard).toContain("book.reading_status === 'completed'");
		expect(bookCard).not.toContain("book.reading_status === 'finished'");
		expect(bookCard).not.toContain('<a href={detailHref}>');
		expect(bookCard).not.toContain('<button class="flex-1 rounded-md bg-accent-500');
		expect(contextMenu).toContain('aria-label={`打开《${book.title}》操作菜单`}');
	});

	it('single-library workspace has accessible tabs, view buttons, and normalized cover URLs', () => {
		const libraryDetailPage = readRoute('src/routes/libraries/[id]/+page.svelte');

		expect(libraryDetailPage).toContain('function normalizeCoverPath');
		expect(libraryDetailPage).toContain('function loadAllLibraryBooks');
		expect(libraryDetailPage).toContain('pageNumber <= totalPages');
		expect(libraryDetailPage).toContain("label: '本库书籍'");
		expect(libraryDetailPage).not.toContain("label: '全部书籍'");
		expect(libraryDetailPage).toContain('src={normalizeCoverPath(book.cover_path)}');
		expect(libraryDetailPage).toContain('role="tablist"');
		expect(libraryDetailPage).toContain('aria-selected={activeTab === tab.key}');
		expect(libraryDetailPage).toContain('aria-label="网格视图"');
		expect(libraryDetailPage).toContain('aria-pressed={booksViewMode ===');
		expect(libraryDetailPage).toContain('aria-label="搜索书名或作者"');
		expect(libraryDetailPage).toContain('aria-label="本库书籍排序"');
		expect(libraryDetailPage).not.toContain('placeholder="搜索书名或作者..."');
	});

	it('single-library workspace guards mobile layout and keyboard affordances', () => {
		const libraryDetailPage = readRoute('src/routes/libraries/[id]/+page.svelte');

		expect(libraryDetailPage).not.toContain('transition-all');
		expect(libraryDetailPage).toContain('class="min-w-0"');
		expect(libraryDetailPage).toContain('title={library.root_path}');
		expect(libraryDetailPage).toContain('aria-label={`筛选本库书籍首字母 ${letter}`}');
		expect(libraryDetailPage).toContain('aria-pressed={activeLetter === letter}');
		expect(libraryDetailPage).toContain('aria-label="清除本库书籍首字母筛选"');
		expect(libraryDetailPage).toContain('aria-label="查看本库全部书籍"');
		expect(libraryDetailPage).toContain('aria-label={`筛选本库系列首字母 ${letter}`}');
		expect(libraryDetailPage).toContain('aria-pressed={seriesLetter === letter}');
		expect(libraryDetailPage).toContain('aria-label="清除本库系列首字母筛选"');
		expect(libraryDetailPage).toContain('aria-label="查看本库全部系列"');
		expect(libraryDetailPage).toContain('width="36"');
		expect(libraryDetailPage).toContain('height="48"');
		expect(libraryDetailPage).toContain('width="48"');
		expect(libraryDetailPage).toContain('height="80"');
	});

	it('single-library workspace owns book upload instead of restoring a global import page', () => {
		const libraryDetailPage = readRoute('src/routes/libraries/[id]/+page.svelte');
		const libraryPage = readRoute('src/routes/library/+page.svelte');
		const importExportLoad = readRoute('src/routes/import-export/+page.ts');

		expect(libraryDetailPage).toContain('api.uploadBook(file, libraryId)');
		expect(libraryDetailPage).toContain('type="file"');
		expect(libraryDetailPage).toContain('accept=".epub,.pdf,.txt,.md,.mobi,.azw3,.docx"');
		expect(libraryDetailPage).toContain('aria-label="向当前书库上传书籍"');
		expect(libraryDetailPage).toContain('上传书籍');
		expect(libraryDetailPage).toContain('let uploadInput = $state<HTMLInputElement | null>(null)');
		expect(libraryPage).not.toContain('api.uploadBook');
		expect(importExportLoad).toContain("redirect(308, '/libraries')");
	});

	it('book detail owns annotation export in the annotations context', () => {
		const bookDetailPage = readRoute('src/routes/library/[id]/+page.svelte');

		expect(bookDetailPage).toContain('api.exportAnnotations(book.id, format)');
		expect(bookDetailPage).toContain("type AnnotationExportFormat = 'markdown' | 'json' | 'notion'");
		expect(bookDetailPage).toContain("handleExportAnnotations('markdown')");
		expect(bookDetailPage).toContain("handleExportAnnotations('json')");
		expect(bookDetailPage).toContain('aria-label="导出 Markdown 批注"');
		expect(bookDetailPage).toContain('aria-label="导出 JSON 批注"');
		expect(bookDetailPage).toContain("{:else if activeTab === 'annotations'}");
	});

	it('reader turns backed bookmark APIs into a full manage and jump workflow', () => {
		const readerStore = readRoute('src/lib/stores/reader.svelte.ts');
		const readerPage = readRoute('src/routes/reading/[id]/+page.svelte');
		const sidebar = readRoute('src/lib/components/reader/ReaderSidebar.svelte');

		expect(readerStore).toContain('bookmarks = $state.raw');
		expect(readerStore).toContain('api.getBookmarks(bookId).catch(() => [])');
		expect(readerStore).toContain('async addBookmark');
		expect(readerStore).toContain('async deleteBookmark');
		expect(readerStore).toContain('api.deleteBookmark(this.book.id, bookmarkId)');

		expect(readerPage).toContain('readerStore.addBookmark');
		expect(readerPage).toContain('function handleBookmarkSelect');
		expect(readerPage).toContain('onbookmarkselect={handleBookmarkSelect}');
		expect(readerPage).toContain('container.scrollTo({');

		expect(sidebar).toContain("'bookmarks'");
		expect(sidebar).toContain('书签');
		expect(sidebar).toContain('readerStore.bookmarks');
		expect(sidebar).toContain('onbookmarkselect?.(');
		expect(sidebar).toContain('aria-label={`删除书签 ${bookmark.title');
	});

	it('semantic vibe search persists and reuses mounted vibe bookmarks', () => {
		const apiService = readRoute('src/lib/services/api.ts');
		const models = readRoute('src/lib/types/models.ts');
		const semanticTags = readRoute('src/routes/semantic-tags/+page.svelte');
		const semanticTagRoutes = readFileSync(
			resolve(appRoot, '../../crates/nova-api/src/routes/semantic_tags.rs'),
			'utf8'
		);

		expect(apiService).toContain('source_text: data.source_text');
		expect(apiService).toContain('source_book_id: data.source_book_id ?? null');
		expect(apiService).toContain('source_chapter_index: data.source_chapter_index ?? null');
		expect(apiService).not.toContain('results_snapshot');

		expect(models).toContain('source_text: string');
		expect(models).toContain('source_book_id?: Id | null');
		expect(models).toContain('source_chapter_index?: number | null');

		expect(semanticTags).toContain('let vibeBookmarks = $state<VibeBookmark[]>([])');
		expect(semanticTags).toContain('await api.getVibeBookmarks()');
		expect(semanticTags).toContain('await api.saveVibeBookmark');
		expect(semanticTags).toContain('function runVibeBookmark');
		expect(semanticTags).toContain('aria-label="保存当前氛围检索"');
		expect(semanticTags).toContain('已保存氛围');

		expect(semanticTagRoutes).toContain('RETURNING id, name, source_text, source_book_id, source_chapter_index, created_at');
		expect(semanticTagRoutes).not.toContain('{ "id": id, "saved": true }');
	});

	it('book-level semantic markers are productized in book detail and reader workspace', () => {
		const semanticMarkersPanel = readRoute('src/lib/components/analytics/SemanticMarkersPanel.svelte');
		const bookDetailPage = readRoute('src/routes/library/[id]/+page.svelte');
		const readerPanel = readRoute('src/lib/components/reader/ReaderIntelligencePanel.svelte');
		const apiService = readRoute('src/lib/services/api.ts');
		const models = readRoute('src/lib/types/models.ts');

		expect(apiService).toContain('async getBookMarkers(bookId: string): Promise<TagMarker[]>');
		expect(apiService).toContain('tag_profile_id: raw.tag_profile_id ?? raw.profile_id');
		expect(apiService).toContain('content_snippet: raw.content_snippet ?? raw.snippet ??');
		expect(models).toContain('tag_profile_id: Id');
		expect(models).toContain('content_snippet: string');
		expect(models).toContain('similarity_score: number');

		expect(semanticMarkersPanel).toContain('api.getBookMarkers(bookId)');
		expect(semanticMarkersPanel).toContain('api.getSemanticProfiles().catch(() => [])');
		expect(semanticMarkersPanel).toContain('await api.computeBookTags(bookId)');
		expect(semanticMarkersPanel).toContain('aria-label="语义标记片段"');
		expect(semanticMarkersPanel).toContain('function markerHref(marker: TagMarker)');
		expect(semanticMarkersPanel).toContain("params.set('chunk', String(marker.chunk_index))");
		expect(semanticMarkersPanel).toContain("params.set('offset', String(marker.offset))");
		expect(semanticMarkersPanel).toContain('语义标记');

		expect(bookDetailPage).toContain("import SemanticMarkersPanel from '$components/analytics/SemanticMarkersPanel.svelte'");
		expect(bookDetailPage).toContain('<SemanticMarkersPanel {bookId} />');
		expect(readerPanel).toContain("import SemanticMarkersPanel from '$components/analytics/SemanticMarkersPanel.svelte'");
		expect(readerPanel).toContain('currentChapterIndex={chapterIndex}');
		expect(readerPanel).toContain('compact');
	});

	it('chapter entities and timelines form a reader to character detail loop', () => {
		const apiService = readRoute('src/lib/services/api.ts');
		const readerContent = readRoute('src/lib/components/reader/ReaderContent.svelte');
		const characterDetail = readRoute('src/routes/characters/[id]/+page.svelte');

		expect(apiService).toContain('async getChapterEntities(bookId: string, index: number)');
		expect(apiService).toContain('async getEntityTimeline(entityId: string)');
		expect(apiService).toContain("'timeline' in result");
		expect(readerContent).toContain('role="link" tabindex="0" data-entity-id');
		expect(readerContent).toContain("goto(`/characters/${entityId}`)");
		expect(readerContent).toContain('function handleEntityKeydown');
		expect(readerContent).toContain("node.addEventListener('keydown', handleEntityKeydown)");
		expect(characterDetail).toContain('api.getEntityTimeline(id).catch(() => [])');
		expect(characterDetail).toContain('{#each timeline as event');
		expect(characterDetail).not.toContain('{#each mentions as mention, i}');
	});

	it('discovery points semantic exploration to the mounted semantic tag workspace', () => {
		const discoverPage = readRoute('src/routes/discover/+page.svelte');
		const semanticTagsPage = readRoute('src/routes/semantic-tags/+page.svelte');

		expect(discoverPage).toContain('href="/semantic-tags?tab=vibe"');
		expect(discoverPage).toContain('氛围检索');
		expect(semanticTagsPage).toContain('api.getSemanticOverview()');
		expect(semanticTagsPage).toContain('function setActiveTab(tab: SemanticTab)');
		expect(semanticTagsPage).toContain("url.searchParams.set('tab', tab)");
	});

	it('search owns the backed cross-book comparison workflow', () => {
		const searchPage = readRoute('src/routes/search/+page.svelte');
		const apiService = readRoute('src/lib/services/api.ts');
		const backendSearch = readFileSync(
			resolve(appRoot, '../../crates/nova-api/src/routes/search.rs'),
			'utf8'
		);

		expect(apiService).toContain('async searchCrossBook(query: string, bookIds?: string[], limit = 30)');
		expect(searchPage).toContain("'cross-book'");
		expect(searchPage).toContain('api.searchCrossBook(query.trim(), undefined, 36)');
		expect(searchPage).toContain('function normalizeCrossBookResults');
		expect(searchPage).toContain('跨书对比');
		expect(searchPage).toContain('覆盖 {crossBookGroups.length} 本书');
		expect(backendSearch).toContain('.route("/search/cross-book", post(search_cross_book))');
		expect(backendSearch).toContain('ensure_book_filters_access');
		expect(backendSearch).toContain('"chunk_id": hit.get("chunk_id").or_else(|| hit.get("id"))');
	});

	it('collections owns smart shelf creation and preview workflows', () => {
		const collectionsPage = readRoute('src/routes/collections/+page.svelte');
		const dragDropShelf = readRoute('src/lib/components/DragDropShelf.svelte');
		const apiService = readRoute('src/lib/services/api.ts');
		const backendShelves = readFileSync(
			resolve(appRoot, '../../crates/nova-api/src/routes/shelves.rs'),
			'utf8'
		);

		expect(apiService).toContain('async getSmartShelves(): Promise<SmartShelf[]>');
		expect(apiService).toContain('async createSmartShelf(data: { name: string; description?: string; filter_criteria: Record<string, unknown> }): Promise<SmartShelf>');
		expect(apiService).toContain('async getSmartShelfBooks(shelfId: string): Promise<SmartShelfBook[]>');
		expect(collectionsPage).toContain("queryKey: ['smart-shelves']");
		expect(collectionsPage).toContain('api.createSmartShelf({');
		expect(collectionsPage).toContain('api.getSmartShelfBooks(selectedSmartShelfId ??');
		expect(collectionsPage).toContain('智能书架');
		expect(collectionsPage).toContain("filter_criteria: { reading_status: newSmartStatus }");
		expect(dragDropShelf).toContain('api.reorderShelf(shelfId, bookIds)');
		expect(dragDropShelf).not.toContain('fetch(`/api/shelves/${shelfId}/reorder`');
		expect(backendShelves).toContain('.route("/shelves/smart", get(list_smart_shelves).post(create_smart_shelf))');
		expect(backendShelves).toContain('async fn list_smart_shelves');
	});

	it('remaining backed AI helpers are owned by real workspaces', () => {
		const apiService = readRoute('src/lib/services/api.ts');
		const writingPage = readRoute('src/routes/writing/+page.svelte');
		const translatePage = readRoute('src/routes/translate/+page.svelte');
		const bookDetailPage = readRoute('src/routes/library/[id]/+page.svelte');
		const readerStore = readRoute('src/lib/stores/reader.svelte.ts');

		expect(apiService).toContain('async aiSummarize(text: string');
		expect(apiService).toContain('async aiExtractEntities(text: string');
		expect(apiService).toContain('async aiSuggestTags(title: string');
		expect(apiService).toContain('body: JSON.stringify({ pairs, source_lang: sourceLang, target_lang: targetLang })');
		expect(apiService).toContain('async detectPlotHoles(bookId: string): Promise<PlotHoleReport>');
		expect(apiService).toContain('body: JSON.stringify({ book_id: bookId })');
		expect(apiService).toContain('async generateChapterTitles(bookId: string, chapterIndex: number)');
		expect(apiService).toContain('body: JSON.stringify({ book_id: bookId, chapter_index: chapterIndex })');

		expect(writingPage).toContain('api.aiSummarize(editorContent.slice(0, 12000)');
		expect(writingPage).toContain('api.aiExtractEntities(editorContent.slice(0, 12000)');
		expect(writingPage).toContain('api.aiSuggestTags(');
		expect(writingPage).toContain('api.cleanupForumText(editorContent)');
		expect(writingPage).toContain('aria-label="清理论坛文本"');

		expect(translatePage).toContain('function buildGlossaryPairs');
		expect(translatePage).toContain('api.extractGlossary(pairs, sourceLanguage, targetLanguage)');
		expect(translatePage).toContain('function saveExtractedTerm');
		expect(translatePage).toContain('aria-label="AI 术语候选"');

		expect(bookDetailPage).toContain('api.generateChapterTitles(book.id, chapterIndex)');
		expect(bookDetailPage).toContain('api.detectPlotHoles(book.id)');
		expect(bookDetailPage).toContain('api.summarizeCommunities(book.id)');
		expect(bookDetailPage).toContain('aria-label="情节一致性诊断"');
		expect(bookDetailPage).toContain('aria-label="图谱社群摘要"');

		expect(readerStore).toContain('const chapterMetaPromise: Promise<Chapter | null>');
		expect(readerStore).toContain('api.getChapter(this.book.id, index).catch(() => null)');
	});

	it('persons have a distinct creator detail route instead of falling back to library search', () => {
		const apiService = readRoute('src/lib/services/api.ts');
		const models = readRoute('src/lib/types/models.ts');
		const personsPage = readRoute('src/routes/persons/+page.svelte');
		const personDetail = readRoute('src/routes/persons/[id]/+page.svelte');

		expect(apiService).toContain('async getPerson(id: string): Promise<Person>');
		expect(apiService).toContain('async getPersonBooks(personId: string): Promise<PersonBook[]>');
		expect(models).toContain('export interface PersonBook');
		expect(personsPage).toContain('href="/persons/{person.id}"');
		expect(personsPage).not.toContain('href="/library?q={encodeURIComponent(person.name)}"');
		expect(personDetail).toContain('api.getPerson(personId)');
		expect(personDetail).toContain('api.getPersonBooks(personId)');
		expect(personDetail).toContain('关联作品');
	});

	it('glossary creation is wired into translation and reader dictionary workflows', () => {
		const apiService = readRoute('src/lib/services/api.ts');
		const models = readRoute('src/lib/types/models.ts');
		const translatePage = readRoute('src/routes/translate/+page.svelte');
		const dictionaryTooltip = readRoute('src/lib/components/reader/DictionaryTooltip.svelte');
		const readerContent = readRoute('src/lib/components/reader/ReaderContent.svelte');
		const immersiveTranslation = readRoute('src/lib/components/reader/ImmersiveTranslation.svelte');
		const selectionMenu = readRoute('src/lib/components/reader/SelectionContextMenu.svelte');

		expect(models).toContain('export interface CreateGlossaryEntryInput');
		expect(models).toContain('export interface AppliedGlossaryMatch');
		expect(apiService).toContain('async createGlossaryEntry(data: CreateGlossaryEntryInput)');
		expect(apiService).toContain('Promise<TranslateTextResult>');
		expect(apiService).toContain('term: data.term');
		expect(apiService).toContain('definition: data.definition ??');
		expect(translatePage).toContain('await api.createGlossaryEntry({');
		expect(translatePage).toContain('let glossaryApplied = $state.raw<AppliedGlossaryMatch[]>');
		expect(translatePage).toContain('{term.source_term}');
		expect(translatePage).toContain('{term.target_term}');
		expect(translatePage).toContain('showGlossaryForm');
		expect(translatePage).toContain('translate-glossary-term');
		expect(dictionaryTooltip).toContain('await api.createGlossaryEntry({');
		expect(dictionaryTooltip).toContain('book_id: bookId ?? null');
		expect(immersiveTranslation).toContain('book_id: bookId');
		expect(selectionMenu).toContain('book_id: bookId');
		expect(readerContent).toContain('<DictionaryTooltip');
		expect(readerContent).toContain('{bookId}');
	});

	it('book custom fields are editable from the book detail information tab', () => {
		const apiService = readRoute('src/lib/services/api.ts');
		const bookDetailPage = readRoute('src/routes/library/[id]/+page.svelte');

		expect(apiService).toContain('async getBookCustomFields(bookId: string): Promise<Record<string, unknown>>');
		expect(apiService).toContain('async updateBookCustomFields(bookId: string, fields: Record<string, unknown>)');
		expect(bookDetailPage).toContain('api.getBookCustomFields(bookId).catch(() => ({}))');
		expect(bookDetailPage).toContain('await api.updateBookCustomFields(book.id, fields)');
		expect(bookDetailPage).toContain('fields[key] = null');
		expect(bookDetailPage).toContain('aria-label="书籍自定义字段"');
		expect(bookDetailPage).toContain('添加字段');
		expect(bookDetailPage).toContain('保存字段');
	});

	it('book detail edit modal keeps form labels and dialog controls accessible', () => {
		const bookDetailPage = readRoute('src/routes/library/[id]/+page.svelte');

		expect(bookDetailPage).toContain('role="dialog"');
		expect(bookDetailPage).toContain('aria-modal="true"');
		expect(bookDetailPage).toContain('tabindex="-1"');
		expect(bookDetailPage).toContain('aria-labelledby="book-edit-title"');
		expect(bookDetailPage).toContain('id="book-edit-title"');
		expect(bookDetailPage).toContain('for="book-edit-title-input"');
		expect(bookDetailPage).toContain('id="book-edit-title-input"');
		expect(bookDetailPage).toContain('for="book-edit-author-input"');
		expect(bookDetailPage).toContain('id="book-edit-author-input"');
		expect(bookDetailPage).toContain('for="book-edit-description-input"');
		expect(bookDetailPage).toContain('id="book-edit-description-input"');
		expect(bookDetailPage).toContain('for="book-edit-reading-status-input"');
		expect(bookDetailPage).toContain('id="book-edit-reading-status-input"');
		expect(bookDetailPage).toContain('for="book-edit-language-input"');
		expect(bookDetailPage).toContain('id="book-edit-language-input"');
		expect(bookDetailPage).toContain('for="book-edit-genres-input"');
		expect(bookDetailPage).toContain('id="book-edit-genres-input"');
		expect(bookDetailPage).toContain('for="book-edit-tags-input"');
		expect(bookDetailPage).toContain('id="book-edit-tags-input"');
	});

	it('global navigation copy and mobile controls stay accessible', () => {
		const topBar = readRoute('src/lib/components/layout/TopBar.svelte');
		const commandPalette = readRoute('src/lib/components/CommandPalette.svelte');
		const sidebar = readRoute('src/lib/components/layout/Sidebar.svelte');
		const libraryHeader = readRoute('src/lib/components/library/LibraryHeader.svelte');
		const bookGrid = readRoute('src/lib/components/library/BookGrid.svelte');

		expect(topBar).toContain('aria-label="打开全局搜索"');
		expect(topBar).toContain('aria-label="打开用户菜单"');
		expect(commandPalette).toContain("id: 'nav-library', label: '所有书籍'");
		expect(commandPalette).toContain("id: 'nav-libraries', label: '书库管理'");
		expect(commandPalette).toContain("id: 'nav-collections', label: '书单'");
		expect(commandPalette).toContain("description: '书单与智能书架'");
		expect(commandPalette).toContain('placeholder="输入命令或搜索…"');
		expect(commandPalette).toContain("id: 'action-scan', label: '选择书库扫描'");
		expect(commandPalette).toContain("action: () => goto('/libraries')");
		expect(commandPalette).not.toContain('api.triggerLibraryScan');
		expect(commandPalette).not.toContain('<svelte:component');
		expect(sidebar).toContain('aria-label="新建书库"');
		expect(sidebar).toContain('我的书库');
		expect(sidebar).toContain("{ href: '/collections', label: '书单'");
		expect(libraryHeader).toContain('sm:flex-row');
		expect(libraryHeader).toContain('书库管理');
		expect(bookGrid).toContain('href="/libraries"');
		expect(bookGrid).not.toContain('href="/admin"');
	});

	it('keyboard shortcut help only advertises implemented global navigation chords', () => {
		const shortcuts = readRoute('src/lib/components/KeyboardShortcuts.svelte');

		expect(shortcuts).toContain("import { goto } from '$app/navigation'");
		expect(shortcuts).toContain("goto('/library')");
		expect(shortcuts).toContain("goto('/libraries')");
		expect(shortcuts).toContain("goto('/search')");
		expect(shortcuts).not.toContain("description: '前往统计'");
	});

	it('admin users page owns group metadata and permission template maintenance', () => {
		const adminUsers = readRoute('src/routes/admin/users/+page.svelte');
		const apiService = readRoute('src/lib/services/api.ts');

		expect(apiService).toContain('async updateGroup(id: string');
		expect(apiService).toContain('async createPermissionTemplate');
		expect(apiService).toContain('async updatePermissionTemplate');
		expect(apiService).toContain('async deletePermissionTemplate');

		expect(adminUsers).toContain("queryKey: ['admin', 'permission-templates']");
		expect(adminUsers).toContain('api.updateGroup(id, {');
		expect(adminUsers).toContain('api.createPermissionTemplate({');
		expect(adminUsers).toContain('api.updatePermissionTemplate(id, {');
		expect(adminUsers).toContain('api.deletePermissionTemplate(id)');
		expect(adminUsers).toContain('权限模板');
		expect(adminUsers).toContain('openGroupEditor(group)');
		expect(adminUsers).toContain('选择 {color} 用户组颜色');
	});

	it('primary navigation does not advertise unfinished global import surfaces', () => {
		const topBar = readRoute('src/lib/components/layout/TopBar.svelte');
		const commandPalette = readRoute('src/lib/components/CommandPalette.svelte');
		const shortcuts = readRoute('src/lib/components/KeyboardShortcuts.svelte');

		expect(topBar).not.toContain("goto('/import-export')");
		expect(commandPalette).not.toContain("goto('/library/import')");
		expect(commandPalette).not.toContain("goto('/import-export')");
		expect(shortcuts).not.toContain('导入新书');
		expect(shortcuts).toContain('aria-label="关闭快捷键帮助"');
		expect(shortcuts).toContain('aria-modal="true"');
		expect(shortcuts).toContain('tabindex="-1"');
	});

	it('legacy global import routes redirect to the library management owner', () => {
		const libraryImportLoad = resolve(appRoot, 'src/routes/library/import/+page.ts');
		const libraryImportPage = resolve(appRoot, 'src/routes/library/import/+page.svelte');
		const importExportLoad = resolve(appRoot, 'src/routes/import-export/+page.ts');
		const importExportPage = resolve(appRoot, 'src/routes/import-export/+page.svelte');

		expect(existsSync(libraryImportLoad)).toBe(true);
		expect(existsSync(importExportLoad)).toBe(true);
		expect(readFileSync(libraryImportLoad, 'utf8')).toContain("redirect(308, '/libraries')");
		expect(readFileSync(importExportLoad, 'utf8')).toContain("redirect(308, '/libraries')");
		expect(existsSync(libraryImportPage)).toBe(false);
		expect(existsSync(importExportPage)).toBe(false);
	});

	it('legacy library management route redirects without keeping duplicate UI', () => {
		const legacyManageLoad = resolve(appRoot, 'src/routes/library/manage/[id]/+page.ts');
		const legacyManagePage = resolve(appRoot, 'src/routes/library/manage/[id]/+page.svelte');

		expect(readFileSync(legacyManageLoad, 'utf8')).toContain("redirect(308, `/libraries/${params.id}/edit`)");
		expect(existsSync(legacyManagePage)).toBe(false);
	});

	it('/recommendations redirects to /discover instead of maintaining a duplicate recommendation UI', () => {
		const redirectPath = resolve(appRoot, 'src/routes/recommendations/+page.ts');
		const legacyPagePath = resolve(appRoot, 'src/routes/recommendations/+page.svelte');

		expect(existsSync(redirectPath)).toBe(true);
		expect(readFileSync(redirectPath, 'utf8')).toContain("redirect(308, '/discover')");
		expect(existsSync(legacyPagePath)).toBe(false);
	});

	it('/duplicates redirects to the library-scoped duplicate workflow', () => {
		const redirectPath = resolve(appRoot, 'src/routes/duplicates/+page.ts');
		const legacyPagePath = resolve(appRoot, 'src/routes/duplicates/+page.svelte');
		const duplicatePage = readRoute('src/routes/library/duplicates/+page.svelte');
		const queryHelpers = readRoute('src/lib/queries/index.ts');

		expect(existsSync(redirectPath)).toBe(true);
		expect(readFileSync(redirectPath, 'utf8')).toContain("redirect(308, '/library/duplicates')");
		expect(existsSync(legacyPagePath)).toBe(false);
		expect(duplicatePage).toContain('api.getDuplicatePairs');
		expect(duplicatePage).toContain('api.getLatestDuplicateScan');
		expect(duplicatePage).toContain('api.startDuplicateScan');
		expect(duplicatePage).toContain('api.resolveDuplicatePair');
		expect(duplicatePage).toContain('const pairs = createQuery(() => ({');
		expect(duplicatePage).toContain('const resolvePair = createMutation(() => ({');
		expect(queryHelpers).not.toContain('useDuplicates');
	});

	it('mobile navigation exposes the single discovery entry', () => {
		const mobileNav = readRoute('src/lib/components/layout/MobileNav.svelte');

		expect(mobileNav).toContain("href: '/discover'");
		expect(mobileNav).toContain("label: '探索'");
		expect(mobileNav).toContain("href: '/libraries'");
		expect(mobileNav).toContain("label: '书库管理'");
		expect(mobileNav).toContain("label: '所有书籍'");
		expect(mobileNav).not.toContain("href: '/stats'");
		expect(mobileNav).not.toContain("href: '/admin'");
	});

	it('single-library routes fetch the addressed library directly', () => {
		const apiService = readRoute('src/lib/services/api.ts');
		const libraryDetailPage = readRoute('src/routes/libraries/[id]/+page.svelte');
		const libraryEditPage = readRoute('src/routes/libraries/[id]/edit/+page.svelte');
		const libraryAnalyzePage = readRoute('src/routes/libraries/[id]/analyze/+page.svelte');

		expect(apiService).toContain('async getLibrary(id: string): Promise<Library>');
		expect(apiService).toContain('return this.request(`/libraries/${id}`)');
		expect(libraryDetailPage).toContain('api.getLibrary(libraryId)');
		expect(libraryEditPage).toContain('api.getLibrary(libraryId)');
		expect(libraryAnalyzePage).toContain('api.getLibrary(libraryId)');
		expect(libraryDetailPage).not.toContain('api.getLibraries().then');
		expect(libraryEditPage).not.toContain('api.getLibraries()');
		expect(libraryAnalyzePage).not.toContain('api.getLibraries().then');
	});

	it('discovery naming is normalized to 探索', () => {
		const topBar = readRoute('src/lib/components/layout/TopBar.svelte');
		const commandPalette = readRoute('src/lib/components/CommandPalette.svelte');
		const dashboardRecommendations = readRoute('src/lib/components/dashboard/Recommendations.svelte');
		const i18n = readRoute('src/lib/i18n/index.ts');
		const zhMessages = readRoute('messages/zh.json');
		const enMessages = readRoute('messages/en.json');

		expect(topBar).not.toContain("'/recommendations'");
		expect(topBar).not.toContain("'/import-export'");
		expect(topBar).toContain("'/discover': '探索'");
		expect(topBar).toContain("'/library/duplicates': '重复检测'");
		expect(topBar).toContain("'/library/batch-edit': '批量编辑'");
		expect(topBar).not.toContain("'/duplicates': '重复检测'");
		expect(commandPalette).toContain("id: 'nav-discover', label: '探索'");
		expect(i18n).toContain("'nav.discover': '探索'");
		expect(i18n).toContain("'nav.library': '所有书籍'");
		expect(i18n).toContain("'nav.libraries': '书库管理'");
		expect(zhMessages).toContain('"nav_library": "所有书籍"');
		expect(zhMessages).toContain('"nav_libraries": "书库管理"');
		expect(zhMessages).toContain('"nav_discover": "探索"');
		expect(enMessages).toContain('"nav_library": "All Books"');
		expect(enMessages).toContain('"nav_libraries": "Library Management"');
		expect(dashboardRecommendations).toContain('暂无探索内容');
		expect(dashboardRecommendations).not.toContain('暂无推荐');
	});

	it('reader and scan affordances use the shared API wrapper instead of raw fetch envelopes', () => {
		const dictionaryTooltip = readRoute('src/lib/components/reader/DictionaryTooltip.svelte');
		const scanStatus = readRoute('src/lib/components/library/LibraryScanStatus.svelte');
		const apiService = readRoute('src/lib/services/api.ts');

		expect(dictionaryTooltip).toContain('api.lookupGlossaryTerm');
		expect(dictionaryTooltip).toContain('type="button"');
		expect(dictionaryTooltip).toContain('aria-label="关闭词典释义"');
		expect(dictionaryTooltip).not.toContain('fetch(`/api/glossary/lookup');
		expect(scanStatus).toContain('api.getLibraryScanStatus');
		expect(scanStatus).toContain('api.scanLibrary');
		expect(scanStatus).not.toContain('fetch(`/api/libraries/');
		expect(apiService).not.toContain('startLibraryScan');
	});

	it('reader annotations expose the mounted public sharing flow', () => {
		const readerSidebar = readRoute('src/lib/components/reader/ReaderSidebar.svelte');
		const annotationShare = readRoute('src/lib/components/AnnotationShare.svelte');
		const sharedAnnotationPage = readRoute('src/routes/share/annotation/[id]/+page.svelte');

		expect(readerSidebar).toContain("import AnnotationShare from '$components/AnnotationShare.svelte'");
		expect(readerSidebar).toContain('<AnnotationShare');
		expect(readerSidebar).toContain('annotationId={ann.id}');
		expect(readerSidebar).toContain('showPreview={false}');
		expect(annotationShare).toContain('api.shareAnnotation(annotationId)');
		expect(annotationShare).toContain('/share/annotation/${data.token}');
		expect(annotationShare).toContain('aria-label="复制分享链接"');
		expect(sharedAnnotationPage).toContain('api.getSharedAnnotation(annotationId)');
	});

	it('discover category filters stay available on mobile', () => {
		const discoverPage = readRoute('src/routes/discover/+page.svelte');

		expect(discoverPage).toContain('overflow-x-auto');
		expect(discoverPage).toContain('role="tablist"');
		expect(discoverPage).toContain('aria-label="探索分类"');
		expect(discoverPage).toContain('aria-selected={activeCategory === category}');
		expect(discoverPage).not.toContain('aria-pressed={activeCategory');
		expect(discoverPage).toContain("params.set('category', activeCategory)");
		expect(discoverPage).toContain('width="80"');
		expect(discoverPage).not.toContain('transition-all');
		expect(discoverPage).not.toContain('hidden gap-1 rounded-lg bg-ink-900/50 p-1 md:flex');
	});

	it('discover lands the mounted reading queue instead of leaving a dead API helper', () => {
		const discoverPage = readRoute('src/routes/discover/+page.svelte');

		expect(discoverPage).toContain('api.getReadingQueue');
		expect(discoverPage).toContain('type ReadingQueueItem');
		expect(discoverPage).toContain('aria-label="阅读队列"');
		expect(discoverPage).toContain('href="/library/{item.id}"');
		expect(discoverPage).toContain('{queueTotal} 本');
	});

	it('/discover includes browse entry points and user-facing exploration language', () => {
		const discoverPage = readRoute('src/routes/discover/+page.svelte');

		expect(discoverPage).toContain('aria-label="探索入口"');
		expect(discoverPage).toContain('href="/library"');
		expect(discoverPage).toContain('href="/series"');
		expect(discoverPage).toContain('href="/tags"');
		expect(discoverPage).toContain('href="/search"');
		expect(discoverPage).toContain('暂无探索内容');
		expect(discoverPage).toContain('aria-label="打开所有书籍补充探索数据"');
		expect(discoverPage).toContain('aria-label="打开语义搜索探索内容"');
		expect(discoverPage).not.toContain('暂无推荐');
	});

	it('primary all-books workflows do not expose placeholder destructive actions', () => {
		const libraryPage = readRoute('src/routes/library/+page.svelte');

		expect(libraryPage).not.toContain('开发中');
		expect(libraryPage).not.toContain('批量删除功能开发中');
		expect(libraryPage).not.toContain('toast.info');
	});

	it('admin subnavigation stays available on mobile', () => {
		const adminLayout = readRoute('src/routes/admin/+layout.svelte');

		expect(adminLayout).toContain('aria-label="管理导航"');
		expect(adminLayout).toContain('md:hidden');
		expect(adminLayout).toContain('overflow-x-auto');
		expect(adminLayout).toContain('href={item.href}');
	});

	it('admin OPDS is not advertised while backend routes are unmounted', () => {
		const adminLayout = readRoute('src/routes/admin/+layout.svelte');
		const opdsPage = readRoute('src/routes/admin/opds/+page.svelte');

		expect(adminLayout).not.toContain('/admin/opds');
		expect(opdsPage).toContain('OPDS 暂未启用');
		expect(opdsPage).not.toContain('运行中');
		expect(opdsPage).not.toContain('opdsUrl');
	});

	it('admin logs route is implemented when advertised', () => {
		const adminLayout = readRoute('src/routes/admin/+layout.svelte');
		const adminPage = readRoute('src/routes/admin/+page.svelte');
		const logsPagePath = resolve(appRoot, 'src/routes/admin/logs/+page.svelte');

		expect(adminLayout).toContain('/admin/logs');
		expect(adminPage).toContain('/admin/logs');
		expect(existsSync(logsPagePath)).toBe(true);
		const logsPage = readFileSync(logsPagePath, 'utf8');
		expect(logsPage).toContain('api.getSystemLogs');
		expect(logsPage).toContain('name="admin-log-level"');
		expect(logsPage).toContain('aria-label="日志级别筛选"');
	});

	it('admin dashboard does not expose the stub global scan endpoint', () => {
		const adminPage = readRoute('src/routes/admin/+page.svelte');
		const apiService = readRoute('src/lib/services/api.ts');

		expect(adminPage).not.toContain('api.triggerLibraryScan');
		expect(adminPage).not.toContain('onclick={triggerScan}');
		expect(adminPage).toContain('href="/libraries"');
		expect(apiService).not.toContain('triggerLibraryScan');
		expect(apiService).not.toContain("'/admin/scan'");
	});

	it('all-books header names search and icon-only view controls', () => {
		const libraryHeader = readRoute('src/lib/components/library/LibraryHeader.svelte');

		expect(libraryHeader).toContain('aria-label="筛选书籍"');
		expect(libraryHeader).toContain('aria-label="网格视图"');
		expect(libraryHeader).toContain('aria-label="列表视图"');
		expect(libraryHeader).toContain('aria-label="表格视图"');
		expect(libraryHeader).toContain('aria-label="时间轴视图"');
		expect(libraryHeader).toContain('aria-pressed={viewMode ===');
	});

	it('all-books quick filters expose pressed state', () => {
		const libraryPage = readRoute('src/routes/library/+page.svelte');
		const libraryFilters = readRoute('src/lib/components/library/LibraryFilters.svelte');
		const bookGrid = readRoute('src/lib/components/library/BookGrid.svelte');

		expect(libraryPage).toContain('aria-pressed={selectionMode}');
		expect(libraryPage).toContain('aria-pressed={filterFormat === fmt}');
		expect(libraryPage).toContain('aria-pressed={filterStatus === v}');
		expect(libraryFilters).toContain('aria-label="书籍排序"');
		expect(libraryFilters).toContain('aria-label="阅读状态筛选"');
		expect(libraryFilters).toContain('aria-label="语言筛选"');
		expect(libraryFilters).toContain('aria-label="格式筛选"');
		expect(bookGrid).toContain('let sentinel = $state<HTMLDivElement | null>(null)');
		expect(bookGrid).toContain('aria-label="选择当前表格页书籍"');
		expect(bookGrid).not.toContain('onclick={(e) => e.stopPropagation()}');
	});
});
