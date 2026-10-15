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
  // MariaDB：与 MySQL 同族（关系型圆柱），中间用波浪线而非分段直线区分
  // （吉祥物为海豹，取“海”的意象；不复制 MySQL 的分段圆柱，避免二者难以辨认）
  mariadb:
    '<svg class="item-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'
    + '<ellipse cx="8" cy="3.6" rx="5.4" ry="2.1"/>'
    + '<path d="M2.6 3.6v8.8c0 1.16 2.42 2.1 5.4 2.1s5.4-.94 5.4-2.1V3.6"/>'
    + '<path d="M4.1 8.5c1.2-1.7 2.4-1.7 3.6 0s2.4 1.7 3.6 0"/>'
    + '</svg>',
  // PostgreSQL：分层堆叠（对象/关系型）
  postgres:
    '<svg class="item-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'
    + '<rect x="3" y="3" width="10" height="4.2" rx="2.1"/>'
    + '<rect x="3" y="8.8" width="10" height="4.2" rx="2.1"/>'
    + '</svg>',
  // SQLite：羽毛（本地嵌入式文件库）
  sqlite:
    '<svg class="item-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'
    + '<path d="M12.6 3.4C9.2 3.6 6.8 5.4 5.7 8.2c-.5 1.3-.8 2.4-.8 2.4s1.2-.4 2.4-.9"/>'
    + '<path d="M12.6 3.4 4.9 12.6"/>'
    + '</svg>',
  // SQL Server：二维网格（表格/服务器）
  sqlserver:
    '<svg class="item-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'
    + '<rect x="2.6" y="2.6" width="10.8" height="10.8" rx="1.6"/>'
    + '<path d="M8 2.6v10.8"/>'
    + '<path d="M2.6 8h10.8"/>'
    + '</svg>',
  // Oracle：椭圆（品牌标志性环形）
  oracle:
    '<svg class="item-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'
    + '<ellipse cx="8" cy="8" rx="5.6" ry="3.3"/>'
    + '</svg>',
  // Redis：钥匙（键值存储）
  redis:
    '<svg class="item-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'
    + '<circle cx="5.6" cy="5.6" r="2.7"/>'
    + '<path d="M7.6 7.6 12.7 12.7"/>'
    + '<path d="M10 10 11.6 8.4"/>'
    + '<path d="M11.6 11.6 13 10.2"/>'
    + '</svg>',
  // Redis key 类型图标：string / list / set / zset / hash / stream，未知类型用键图形
  // string：引号
  'rkey-string':
    '<svg class="item-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'
    + '<path d="M6.3 4.9 5.2 7.6"/>'
    + '<path d="M6.3 4.9c1 0 1.6.8 1.6 2v2.2"/>'
    + '<path d="M10 4.9 8.9 7.6"/>'
    + '<path d="M10 4.9c1 0 1.6.8 1.6 2v2.2"/>'
    + '</svg>',
  // list：项目符号列表
  'rkey-list':
    '<svg class="item-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'
    + '<path d="M3.4 5h.01M3.4 8h.01M3.4 11h.01"/>'
    + '<path d="M6.6 5h6.2M6.6 8h6.2M6.6 11h6.2"/>'
    + '</svg>',
  // set：散落的点（无序）
  'rkey-set':
    '<svg class="item-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'
    + '<circle cx="5.2" cy="5.4" r="1.5"/>'
    + '<circle cx="10.8" cy="5.4" r="1.5"/>'
    + '<circle cx="8" cy="10.6" r="1.5"/>'
    + '</svg>',
  // zset：带分值的升序点（有序）
  'rkey-zset':
    '<svg class="item-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'
    + '<circle cx="4.3" cy="11.3" r="1.5"/>'
    + '<circle cx="8" cy="8" r="1.5"/>'
    + '<circle cx="11.7" cy="4.7" r="1.5"/>'
    + '</svg>',
  // hash：井号
  'rkey-hash':
    '<svg class="item-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'
    + '<path d="M6.2 3.4 5 12.6"/>'
    + '<path d="M11 3.4 9.8 12.6"/>'
    + '<path d="M3.8 6.2h8.6"/>'
    + '<path d="M3.4 9.8h8.6"/>'
    + '</svg>',
  // stream：脉冲折线
  'rkey-stream':
    '<svg class="item-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'
    + '<path d="M2.3 8.5h2.4l1.5-3.7 2 7 1.6-4.6 1.2 2.6h2.7"/>'
    + '</svg>',
  // 未知类型：复用钥匙图形
  'rkey-none':
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
