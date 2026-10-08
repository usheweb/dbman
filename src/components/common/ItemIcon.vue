<script setup>
import { computed } from 'vue';

// 列表项图标：内联 SVG，描边用 currentColor 跟随条目文字颜色（选中变青色时自动跟着变）
const ICONS = {
  database:
    '<svg class="item-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" aria-hidden="true">'
    + '<ellipse cx="8" cy="3.6" rx="5.4" ry="2.1"/>'
    + '<path d="M2.6 3.6v8.8c0 1.16 2.42 2.1 5.4 2.1s5.4-.94 5.4-2.1V3.6"/>'
    + '<path d="M2.6 8c0 1.16 2.42 2.1 5.4 2.1s5.4-.94 5.4-2.1"/>'
    + '</svg>',
  table:
    '<svg class="item-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" aria-hidden="true">'
    + '<rect x="2.2" y="3.2" width="11.6" height="9.6" rx="1.4"/>'
    + '<path d="M2.2 6.4h11.6"/>'
    + '<path d="M6.6 6.4v6.4"/>'
    + '</svg>',
  // 数据库类型图标：自绘简化单色版，不使用厂商注册商标 logo
  // MySQL：三层数据库圆柱（关系型）
  mysql:
    '<svg class="item-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" aria-hidden="true">'
    + '<ellipse cx="8" cy="3.6" rx="5.4" ry="2.1"/>'
    + '<path d="M2.6 3.6v8.8c0 1.16 2.42 2.1 5.4 2.1s5.4-.94 5.4-2.1V3.6"/>'
    + '<path d="M2.6 6.5c0 1.16 2.42 2.1 5.4 2.1s5.4-.94 5.4-2.1"/>'
    + '<path d="M2.6 9.4c0 1.16 2.42 2.1 5.4 2.1s5.4-.94 5.4-2.1"/>'
    + '</svg>',
  // Redis：钥匙（键值存储）
  redis:
    '<svg class="item-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'
    + '<circle cx="5.6" cy="5.6" r="2.7"/>'
    + '<path d="M7.6 7.6 12.7 12.7"/>'
    + '<path d="M10 10 11.6 8.4"/>'
    + '<path d="M11.6 11.6 13 10.2"/>'
    + '</svg>'
};

const props = defineProps({
  name: { type: String, required: true }
});

const svg = computed(() => ICONS[props.name] || '');
</script>

<template>
  <span class="item-icon" v-html="svg"></span>
</template>
