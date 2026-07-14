<script lang="ts">
	import { Monitor, Smartphone, Tablet, Wifi } from 'lucide-svelte';

	let {
		bookId = '',
		currentChapter = 0,
		currentPosition = 0,
	} = $props<{
		bookId?: string;
		currentChapter?: number;
		currentPosition?: number;
	}>();

	interface DeviceSync {
		device: string;
		chapter_index: number;
		scroll_position: number;
		timestamp: number;
	}

	let otherDevices = $state<DeviceSync[]>([]);
	let wsConnected = $state(false);
	let ws: WebSocket | null = null;
	let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
	let disposed = false;

	$effect(() => {
		disposed = false;
		connectWs();
		return () => {
			disposed = true;
			if (reconnectTimer) clearTimeout(reconnectTimer);
			ws?.close();
			ws = null;
		};
	});

	function connectWs() {
		if (disposed) return;
		const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
		ws = new WebSocket(`${protocol}//${window.location.host}/ws/progress`);

		ws.onopen = () => { wsConnected = true; };
		ws.onclose = () => {
			wsConnected = false;
			if (!disposed) {
				reconnectTimer = setTimeout(connectWs, 5000);
			}
		};
		ws.onmessage = (event) => {
			try {
				const msg = JSON.parse(event.data);
				if (msg.type === 'position_sync' && msg.book_id === bookId) {
					// Update or add device entry
					const idx = otherDevices.findIndex(d => d.device === msg.device);
					const entry: DeviceSync = {
						device: msg.device,
						chapter_index: msg.chapter_index,
						scroll_position: msg.scroll_position,
						timestamp: msg.timestamp,
					};
					if (idx >= 0) {
						otherDevices[idx] = entry;
					} else {
						otherDevices = [...otherDevices, entry];
					}
				}
			} catch { /* ignore parse errors */ }
		};
	}

	// Send position updates (debounced by tracking dependencies)
	let positionSendTimer: ReturnType<typeof setTimeout> | null = null;
	$effect(() => {
		const chapter = currentChapter;
		const position = currentPosition;
		const book = bookId;

		if (positionSendTimer) clearTimeout(positionSendTimer);
		positionSendTimer = setTimeout(() => {
			if (ws?.readyState === WebSocket.OPEN && book) {
				ws.send(JSON.stringify({
					type: 'position_update',
					book_id: book,
					chapter_index: chapter,
					scroll_position: position,
					timestamp: Date.now(),
				}));
			}
		}, 500); // debounce 500ms
	});

	const deviceIcons: Record<string, typeof Monitor> = {
		desktop: Monitor,
		mobile: Smartphone,
		tablet: Tablet,
		web: Monitor,
	};

	function formatTime(ts: number): string {
		const diff = Date.now() - ts;
		if (diff < 60000) return '刚刚';
		if (diff < 3600000) return `${Math.floor(diff / 60000)} 分钟前`;
		return `${Math.floor(diff / 3600000)} 小时前`;
	}
</script>

{#if otherDevices.length > 0 || wsConnected}
	<div class="flex items-center gap-2">
		<!-- Connection indicator -->
		<div class="flex items-center gap-1 text-xs {wsConnected ? 'text-green-400' : 'text-ink-500'}">
			<Wifi size={12} />
			<span class="hidden sm:inline">{wsConnected ? '已同步' : '离线'}</span>
		</div>

		<!-- Other devices -->
		{#each otherDevices as device}
			{@const Icon = deviceIcons[device.device] ?? Monitor}
			<div class="flex items-center gap-1 rounded-full bg-ink-800 border border-ink-700 px-2 py-0.5 text-xs text-ink-400" title="第{device.chapter_index + 1}章 · {formatTime(device.timestamp)}">
				<Icon size={10} />
				<span>Ch.{device.chapter_index + 1}</span>
			</div>
		{/each}
	</div>
{/if}
