<script lang="ts">
	import { onMount, onDestroy } from 'svelte';

	let { nodes, edges, onNodeClick, width = 800, height = 600 } = $props<{
		nodes: Array<{ id: string; label: string; type: string; size: number }>;
		edges: Array<{ source: string; target: string; label: string; weight?: number }>;
		onNodeClick?: (nodeId: string) => void;
		width?: number;
		height?: number;
	}>();

	let canvas: HTMLCanvasElement;
	let ctx: CanvasRenderingContext2D | null = null;
	let animationFrame: number;
	let hoveredNode: string | null = $state(null);

	// Physics simulation state
	interface SimNode {
		id: string;
		label: string;
		type: string;
		size: number;
		x: number;
		y: number;
		vx: number;
		vy: number;
	}

	let simNodes: SimNode[] = [];
	let simRunning = true;

	const typeColors: Record<string, string> = {
		person: '#f59e0b',
		location: '#22c55e',
		organization: '#6366f1',
		item: '#ec4899',
		concept: '#8b5cf6',
	};

	function initSimulation() {
		// Place nodes randomly
		simNodes = nodes.map((n: typeof nodes[number]) => ({
			...n,
			x: width / 2 + (Math.random() - 0.5) * width * 0.6,
			y: height / 2 + (Math.random() - 0.5) * height * 0.6,
			vx: 0,
			vy: 0,
		}));
	}

	function simulate() {
		const alpha = 0.3;
		const repulsionStrength = 500;
		const attractionStrength = 0.005;
		const centerStrength = 0.01;
		const damping = 0.85;

		// Center force
		for (const node of simNodes) {
			node.vx += (width / 2 - node.x) * centerStrength;
			node.vy += (height / 2 - node.y) * centerStrength;
		}

		// Repulsion (all pairs)
		for (let i = 0; i < simNodes.length; i++) {
			for (let j = i + 1; j < simNodes.length; j++) {
				const dx = simNodes[j].x - simNodes[i].x;
				const dy = simNodes[j].y - simNodes[i].y;
				const dist = Math.sqrt(dx * dx + dy * dy) || 1;
				const force = repulsionStrength / (dist * dist);
				const fx = (dx / dist) * force;
				const fy = (dy / dist) * force;
				simNodes[i].vx -= fx;
				simNodes[i].vy -= fy;
				simNodes[j].vx += fx;
				simNodes[j].vy += fy;
			}
		}

		// Attraction (edges)
		for (const edge of edges) {
			const source = simNodes.find(n => n.id === edge.source);
			const target = simNodes.find(n => n.id === edge.target);
			if (!source || !target) continue;

			const dx = target.x - source.x;
			const dy = target.y - source.y;
			const dist = Math.sqrt(dx * dx + dy * dy) || 1;
			const force = dist * attractionStrength;
			const fx = (dx / dist) * force;
			const fy = (dy / dist) * force;
			source.vx += fx;
			source.vy += fy;
			target.vx -= fx;
			target.vy -= fy;
		}

		// Apply velocity + damping
		for (const node of simNodes) {
			node.vx *= damping;
			node.vy *= damping;
			node.x += node.vx * alpha;
			node.y += node.vy * alpha;

			// Bounds
			node.x = Math.max(30, Math.min(width - 30, node.x));
			node.y = Math.max(30, Math.min(height - 30, node.y));
		}
	}

	function draw() {
		if (!ctx) return;
		ctx.clearRect(0, 0, width, height);

		// Draw edges
		ctx.strokeStyle = 'rgba(168, 162, 158, 0.15)';
		ctx.lineWidth = 1;
		for (const edge of edges) {
			const source = simNodes.find(n => n.id === edge.source);
			const target = simNodes.find(n => n.id === edge.target);
			if (!source || !target) continue;

			ctx.beginPath();
			ctx.moveTo(source.x, source.y);
			ctx.lineTo(target.x, target.y);
			ctx.stroke();

			// Edge label at midpoint
			if (edge.label) {
				const mx = (source.x + target.x) / 2;
				const my = (source.y + target.y) / 2;
				ctx.fillStyle = 'rgba(168, 162, 158, 0.4)';
				ctx.font = '9px Inter';
				ctx.textAlign = 'center';
				ctx.fillText(edge.label, mx, my - 3);
			}
		}

		// Draw nodes
		for (const node of simNodes) {
			const radius = Math.max(6, Math.min(20, node.size * 3));
			const color = typeColors[node.type] ?? '#78716c';
			const isHovered = hoveredNode === node.id;

			// Node circle
			ctx.beginPath();
			ctx.arc(node.x, node.y, radius + (isHovered ? 3 : 0), 0, Math.PI * 2);
			ctx.fillStyle = isHovered ? color : `${color}88`;
			ctx.fill();

			// Border
			ctx.strokeStyle = color;
			ctx.lineWidth = isHovered ? 2.5 : 1.5;
			ctx.stroke();

			// Label
			ctx.fillStyle = isHovered ? '#fafaf9' : '#d6d3d1';
			ctx.font = `${isHovered ? 'bold ' : ''}11px Inter`;
			ctx.textAlign = 'center';
			ctx.fillText(node.label, node.x, node.y + radius + 14);
		}
	}

	function animate() {
		if (simRunning) {
			simulate();
		}
		draw();
		animationFrame = requestAnimationFrame(animate);
	}

	function handleMouseMove(e: MouseEvent) {
		if (!canvas) return;
		const rect = canvas.getBoundingClientRect();
		const mx = e.clientX - rect.left;
		const my = e.clientY - rect.top;

		hoveredNode = null;
		for (const node of simNodes) {
			const radius = Math.max(6, Math.min(20, node.size * 3));
			const dx = mx - node.x;
			const dy = my - node.y;
			if (dx * dx + dy * dy < (radius + 5) * (radius + 5)) {
				hoveredNode = node.id;
				canvas.style.cursor = 'pointer';
				return;
			}
		}
		canvas.style.cursor = 'default';
	}

	function handleClick(e: MouseEvent) {
		if (hoveredNode && onNodeClick) {
			onNodeClick(hoveredNode);
		}
	}

	onMount(() => {
		ctx = canvas.getContext('2d');
		initSimulation();
		animate();

		// Stop heavy simulation after 3 seconds
		setTimeout(() => { simRunning = false; }, 3000);
	});

	onDestroy(() => {
		if (animationFrame) cancelAnimationFrame(animationFrame);
	});

	// Re-init when data changes
	$effect(() => {
		if (nodes.length > 0) {
			initSimulation();
			simRunning = true;
			setTimeout(() => { simRunning = false; }, 2000);
		}
	});
</script>

<canvas
	bind:this={canvas}
	{width}
	{height}
	class="w-full h-full"
	onmousemove={handleMouseMove}
	onclick={handleClick}
></canvas>
