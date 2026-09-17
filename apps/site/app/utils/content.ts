// 产品页与打印排版（海报、说明书）共用的文案；只在一处改，两边不会说成两套话

export const REPOSITORY = 'https://github.com/x-wink/flair-bloom';
export const SITE_URL = 'https://app.xwink.fun/flair-bloom/';
export const DOWNLOAD_URL = 'https://app.xwink.fun/flair-bloom/download';

export type HighlightIconName = 'lightning' | 'palette' | 'keyboard' | 'speaker';

export interface TitledText {
  title: string;
  text: string;
}

export interface Highlight extends TitledText {
  icon: HighlightIconName;
}

// 玩家群里反复被夸的几点，单独放大，不做评价墙也不署名
export const highlights: Highlight[] = [
  {
    icon: 'lightning',
    title: '有效降低输入延迟',
    text: '技能一转好就按出去，不用盯着 CD 手搓抢时机。连发间隔最低 10ms，比手指狂按密得多。',
  },
  {
    icon: 'palette',
    title: '界面好看，门派色随心配',
    text: '亮暗模式随手换，20 种门派主题色任你搭。',
  },
  {
    icon: 'keyboard',
    title: '右 Alt 也能当热键',
    text: '一键开关、收起窗口这些热键，Shift、Ctrl、Alt、Win 都能绑，左右分开认。顺手的右 Alt 终于用上了，游戏里直接按。',
  },
  {
    icon: 'speaker',
    title: '语音提示自己定',
    text: '开关连发时念一句，不用盯屏幕。台词随便写，比如「我准备好库库按了」；也能换成自己的音频文件，mp3、wav 这些都行。',
  },
];

export const features: TitledText[] = [
  { title: '武学助手 FFF', text: '开了武学助手只管狂按 F？让它替你自动连 F，手指解放。' },
  { title: '一键宏启动', text: '一长串技能宏，按一下就持续触发，不用反复戳。' },
  { title: '手搓循环', text: '输出循环要一直按同一个键时，按住就交给它连，松手就停。' },
  {
    title: '多段宏分组',
    text: '几条切换连发放进同一分组，组内同一时刻只跑一条，按 2 自动停掉 1。',
  },
  {
    title: '所有键都能连',
    text: '键盘、鼠标左右中、两个侧键、滚轮上下随意组合，比如按住侧键连左键。',
  },
  {
    title: '多套配置随手切',
    text: '不同角色、心法各存一套，托盘右键就能切；导出 .qzh 文件分享给朋友。',
  },
];

// 前四条是作者写的游戏梗，后两条摘自玩家群聊（不署名）；区块标题写明「骚话」，不冒充实名评价。
// offset 是桌面端左缩进，offsetSm 是手机端，错开摆放拼出互相压边的效果
export const rumors = [
  { text: '终于找到一个能用的，以前过的都是什么苦日子啊 TT', offset: '0%', offsetSm: '0%' },
  { text: '原来还能换门派色吗？紫色还是最有韵味～', offset: '42%', offsetSm: '10%' },
  { text: '把作者抓起来吧，我怀疑他私藏重器', offset: '18%', offsetSm: '4%' },
  { text: '我劝你们别在测试服用，会影响正式服强度', offset: '4%', offsetSm: '0%' },
  { text: '我最喜欢的右 Alt 终于可以使用了', offset: '38%', offsetSm: '10%' },
  { text: '界面已经遥遥领先了', offset: '14%', offsetSm: '4%' },
];

export const steps: TitledText[] = [
  {
    title: '安装助手',
    text: '下载安装包双击安装，首次打开同意协议。玩游戏请切到「游戏模式」，按提示授权后重启一次。',
  },
  {
    title: '设置规则',
    text: '在「按压连发」页点「+」，点输入框后直接按要连的那个键（键盘、鼠标、滚轮都行），打开右侧开关。',
  },
  {
    title: '打开总开关',
    text: '点右下角「全局已禁用」让它变成「已启用」，按住那个键自动连发，松手停。',
  },
];

export const cautions: string[] = [
  '它靠模拟按键工作，存在被游戏反作弊检测的风险；能不能用、会不会处罚，请自行评估承担。',
  '游戏里请用「游戏模式」，它会装一个小驱动并以管理员运行；个别游戏会拦它，遇到就切回「通用模式」。',
  'Windows 弹出「已保护你的电脑」时，点「更多信息 → 仍要运行」即可。',
];

export const assurances: TitledText[] = [
  { title: '松手就停', text: '多条规则一起连会自动控总速，不会越叠越快、停不下来。' },
  { title: '不动游戏文件', text: '只是替你按键，不改游戏、不读游戏数据。' },
  { title: '配置存在自己电脑', text: '不用注册、不上传，除了检查更新不联网。' },
  { title: '更新省心', text: '新版本自动更新，国内下载也快，安装包都校验过防掉包。' },
];
