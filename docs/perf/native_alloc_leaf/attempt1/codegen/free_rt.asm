   140006f50:	41 57                	push   %r15
   140006f52:	41 56                	push   %r14
   140006f54:	41 55                	push   %r13
   140006f56:	41 54                	push   %r12
   140006f58:	56                   	push   %rsi
   140006f59:	57                   	push   %rdi
   140006f5a:	55                   	push   %rbp
   140006f5b:	53                   	push   %rbx
   140006f5c:	48 83 ec 38          	sub    $0x38,%rsp
   140006f60:	8b 05 6a 0c 06 00    	mov    0x60c6a(%rip),%eax        # 0x140067bd0
   140006f66:	65 4c 8b 0c 25 58 00 	mov    %gs:0x58,%r9
   140006f6d:	00 00 
   140006f6f:	49 8b 04 c1          	mov    (%r9,%rax,8),%rax
   140006f73:	48 8b b0 38 00 00 00 	mov    0x38(%rax),%rsi
   140006f7a:	48 8d 46 ff          	lea    -0x1(%rsi),%rax
   140006f7e:	48 83 f8 fd          	cmp    $0xfffffffffffffffd,%rax
   140006f82:	0f 87 da 03 00 00    	ja     0x140007362
   140006f88:	48 89 c8             	mov    %rcx,%rax
   140006f8b:	48 25 00 00 c0 ff    	and    $0xffffffffffc00000,%rax
   140006f91:	48 39 86 30 01 00 00 	cmp    %rax,0x130(%rsi)
   140006f98:	75 1d                	jne    0x140006fb7
   140006f9a:	48 8b be 38 01 00 00 	mov    0x138(%rsi),%rdi
   140006fa1:	44 0f b6 4f 1c       	movzbl 0x1c(%rdi),%r9d
   140006fa6:	41 83 f9 02          	cmp    $0x2,%r9d
   140006faa:	74 4b                	je     0x140006ff7
   140006fac:	81 e1 ff ff 3f 00    	and    $0x3fffff,%ecx
   140006fb2:	49 89 cf             	mov    %rcx,%r15
   140006fb5:	eb 50                	jmp    0x140007007
   140006fb7:	49 89 c2             	mov    %rax,%r10
   140006fba:	49 c1 ea 16          	shr    $0x16,%r10
   140006fbe:	45 89 d1             	mov    %r10d,%r9d
   140006fc1:	41 83 e1 0f          	and    $0xf,%r9d
   140006fc5:	41 c1 e1 04          	shl    $0x4,%r9d
   140006fc9:	4a 39 44 0e 30       	cmp    %rax,0x30(%rsi,%r9,1)
   140006fce:	0f 85 65 01 00 00    	jne    0x140007139
   140006fd4:	4a 8b 7c 0e 38       	mov    0x38(%rsi,%r9,1),%rdi
   140006fd9:	48 39 c7             	cmp    %rax,%rdi
   140006fdc:	75 0e                	jne    0x140006fec
   140006fde:	48 89 86 30 01 00 00 	mov    %rax,0x130(%rsi)
   140006fe5:	48 89 be 38 01 00 00 	mov    %rdi,0x138(%rsi)
   140006fec:	44 0f b6 4f 1c       	movzbl 0x1c(%rdi),%r9d
   140006ff1:	41 83 f9 02          	cmp    $0x2,%r9d
   140006ff5:	75 b5                	jne    0x140006fac
   140006ff7:	49 89 cf             	mov    %rcx,%r15
   140006ffa:	49 29 ff             	sub    %rdi,%r15
   140006ffd:	4c 3b 7f 38          	cmp    0x38(%rdi),%r15
   140007001:	0f 83 5b 03 00 00    	jae    0x140007362
   140007007:	4a 8d 1c 3f          	lea    (%rdi,%r15,1),%rbx
   14000700b:	49 83 f8 11          	cmp    $0x11,%r8
   14000700f:	b8 10 00 00 00       	mov    $0x10,%eax
   140007014:	49 0f 43 c0          	cmovae %r8,%rax
   140007018:	48 83 fa 11          	cmp    $0x11,%rdx
   14000701c:	73 75                	jae    0x140007093
   14000701e:	48 8d 48 ff          	lea    -0x1(%rax),%rcx
   140007022:	48 81 f9 bf f2 03 00 	cmp    $0x3f2bf,%rcx
   140007029:	0f 87 d1 00 00 00    	ja     0x140007100
   14000702f:	48 c1 e9 04          	shr    $0x4,%rcx
   140007033:	48 8d 05 1e a0 03 00 	lea    0x3a01e(%rip),%rax        # 0x140041058
   14000703a:	44 0f b6 34 01       	movzbl (%rcx,%rax,1),%r14d
   14000703f:	49 83 fe 31          	cmp    $0x31,%r14
   140007043:	0f 83 a2 05 00 00    	jae    0x1400075eb
   140007049:	44 89 f0             	mov    %r14d,%eax
   14000704c:	c1 e0 07             	shl    $0x7,%eax
   14000704f:	4a 8d 0c f0          	lea    (%rax,%r14,8),%rcx
   140007053:	0f b6 84 0e a8 04 00 	movzbl 0x4a8(%rsi,%rcx,1),%eax
   14000705a:	00 
   14000705b:	45 84 c9             	test   %r9b,%r9b
   14000705e:	0f 84 46 03 00 00    	je     0x1400073aa
   140007064:	45 0f b6 c9          	movzbl %r9b,%r9d
   140007068:	41 83 f9 02          	cmp    $0x2,%r9d
   14000706c:	0f 84 00 03 00 00    	je     0x140007372
   140007072:	41 83 f9 01          	cmp    $0x1,%r9d
   140007076:	0f 85 41 05 00 00    	jne    0x1400075bd
   14000707c:	41 b9 00 20 04 00    	mov    $0x42000,%r9d
   140007082:	44 89 fa             	mov    %r15d,%edx
   140007085:	4c 39 ca             	cmp    %r9,%rdx
   140007088:	0f 83 2e 03 00 00    	jae    0x1400073bc
   14000708e:	e9 2a 05 00 00       	jmp    0x1400075bd
   140007093:	48 39 d0             	cmp    %rdx,%rax
   140007096:	49 89 d2             	mov    %rdx,%r10
   140007099:	4c 0f 47 d0          	cmova  %rax,%r10
   14000709d:	49 ff ca             	dec    %r10
   1400070a0:	49 81 fa bf f2 03 00 	cmp    $0x3f2bf,%r10
   1400070a7:	77 57                	ja     0x140007100
   1400070a9:	49 c1 ea 04          	shr    $0x4,%r10
   1400070ad:	48 8d 0d fc f2 03 00 	lea    0x3f2fc(%rip),%rcx        # 0x1400463b0
   1400070b4:	45 0f b6 9c 0a 94 01 	movzbl 0x194(%r10,%rcx,1),%r11d
   1400070bb:	00 00 
   1400070bd:	41 80 fb 30          	cmp    $0x30,%r11b
   1400070c1:	77 3d                	ja     0x140007100
   1400070c3:	4c 8d 52 ff          	lea    -0x1(%rdx),%r10
   1400070c7:	66 0f 1f 84 00 00 00 	nopw   0x0(%rax,%rax,1)
   1400070ce:	00 00 
   1400070d0:	45 0f b6 f3          	movzbl %r11b,%r14d
   1400070d4:	4e 8b 1c f1          	mov    (%rcx,%r14,8),%r11
   1400070d8:	4d 85 d3             	test   %r10,%r11
   1400070db:	0f 84 68 ff ff ff    	je     0x140007049
   1400070e1:	4d 09 d3             	or     %r10,%r11
   1400070e4:	49 81 fb bf f2 03 00 	cmp    $0x3f2bf,%r11
   1400070eb:	77 13                	ja     0x140007100
   1400070ed:	49 c1 eb 04          	shr    $0x4,%r11
   1400070f1:	45 0f b6 9c 0b 94 01 	movzbl 0x194(%r11,%rcx,1),%r11d
   1400070f8:	00 00 
   1400070fa:	41 80 fb 31          	cmp    $0x31,%r11b
   1400070fe:	72 d0                	jb     0x1400070d0
   140007100:	41 83 f9 02          	cmp    $0x2,%r9d
   140007104:	0f 83 e0 00 00 00    	jae    0x1400071ea
   14000710a:	48 83 fa 11          	cmp    $0x11,%rdx
   14000710e:	0f 83 f2 00 00 00    	jae    0x140007206
   140007114:	48 ff c8             	dec    %rax
   140007117:	48 3d bf f2 03 00    	cmp    $0x3f2bf,%rax
   14000711d:	0f 87 4e 01 00 00    	ja     0x140007271
   140007123:	48 c1 e8 04          	shr    $0x4,%rax
   140007127:	48 8d 0d 2a 9f 03 00 	lea    0x39f2a(%rip),%rcx        # 0x140041058
   14000712e:	0f b6 04 08          	movzbl (%rax,%rcx,1),%eax
   140007132:	b1 01                	mov    $0x1,%cl
   140007134:	e9 3a 01 00 00       	jmp    0x140007273
   140007139:	49 01 f1             	add    %rsi,%r9
   14000713c:	49 83 c1 30          	add    $0x30,%r9
   140007140:	49 89 c3             	mov    %rax,%r11
   140007143:	49 c1 eb 23          	shr    $0x23,%r11
   140007147:	48 89 c7             	mov    %rax,%rdi
   14000714a:	48 c1 ef 30          	shr    $0x30,%rdi
   14000714e:	41 31 fb             	xor    %edi,%r11d
   140007151:	45 31 da             	xor    %r11d,%r10d
   140007154:	41 81 e2 ff 1f 00 00 	and    $0x1fff,%r10d
   14000715b:	4c 8b 9e 40 01 00 00 	mov    0x140(%rsi),%r11
   140007162:	4c 89 d3             	mov    %r10,%rbx
   140007165:	66 66 2e 0f 1f 84 00 	data16 cs nopw 0x0(%rax,%rax,1)
   14000716c:	00 00 00 00 
   140007170:	49 8b 3c db          	mov    (%r11,%rbx,8),%rdi
   140007174:	48 85 ff             	test   %rdi,%rdi
   140007177:	0f 84 e5 01 00 00    	je     0x140007362
   14000717d:	49 89 fe             	mov    %rdi,%r14
   140007180:	49 81 e6 00 00 c0 ff 	and    $0xffffffffffc00000,%r14
   140007187:	49 39 c6             	cmp    %rax,%r14
   14000718a:	74 12                	je     0x14000719e
   14000718c:	ff c3                	inc    %ebx
   14000718e:	81 e3 ff 1f 00 00    	and    $0x1fff,%ebx
   140007194:	4c 39 d3             	cmp    %r10,%rbx
   140007197:	75 d7                	jne    0x140007170
   140007199:	e9 c4 01 00 00       	jmp    0x140007362
   14000719e:	49 89 fa             	mov    %rdi,%r10
   1400071a1:	49 81 e2 ff ff 3f 00 	and    $0x3fffff,%r10
   1400071a8:	74 25                	je     0x1400071cf
   1400071aa:	4d 8d 5a ff          	lea    -0x1(%r10),%r11
   1400071ae:	8b be 58 01 00 00    	mov    0x158(%rsi),%edi
   1400071b4:	49 39 fb             	cmp    %rdi,%r11
   1400071b7:	0f 83 a5 01 00 00    	jae    0x140007362
   1400071bd:	4c 8b 5e 20          	mov    0x20(%rsi),%r11
   1400071c1:	4b 8b 7c d3 f8       	mov    -0x8(%r11,%r10,8),%rdi
   1400071c6:	48 85 ff             	test   %rdi,%rdi
   1400071c9:	0f 84 93 01 00 00    	je     0x140007362
   1400071cf:	49 89 01             	mov    %rax,(%r9)
   1400071d2:	49 89 79 08          	mov    %rdi,0x8(%r9)
   1400071d6:	44 0f b6 4f 1c       	movzbl 0x1c(%rdi),%r9d
   1400071db:	41 83 f9 02          	cmp    $0x2,%r9d
   1400071df:	0f 84 12 fe ff ff    	je     0x140006ff7
   1400071e5:	e9 c2 fd ff ff       	jmp    0x140006fac
   1400071ea:	0f 85 cd 03 00 00    	jne    0x1400075bd
   1400071f0:	48 89 f1             	mov    %rsi,%rcx
   1400071f3:	48 89 da             	mov    %rbx,%rdx
   1400071f6:	4c 89 c6             	mov    %r8,%rsi
   1400071f9:	49 89 f8             	mov    %rdi,%r8
   1400071fc:	e8 bf 42 03 00       	call   0x14003b4c0
   140007201:	e9 64 01 00 00       	jmp    0x14000736a
   140007206:	48 39 d0             	cmp    %rdx,%rax
   140007209:	48 0f 46 c2          	cmovbe %rdx,%rax
   14000720d:	48 ff c8             	dec    %rax
   140007210:	48 3d bf f2 03 00    	cmp    $0x3f2bf,%rax
   140007216:	77 59                	ja     0x140007271
   140007218:	48 c1 e8 04          	shr    $0x4,%rax
   14000721c:	4c 8d 15 8d f1 03 00 	lea    0x3f18d(%rip),%r10        # 0x1400463b0
   140007223:	46 0f b6 9c 10 94 01 	movzbl 0x194(%rax,%r10,1),%r11d
   14000722a:	00 00 
   14000722c:	41 80 fb 30          	cmp    $0x30,%r11b
   140007230:	77 3f                	ja     0x140007271
   140007232:	48 ff ca             	dec    %rdx
   140007235:	66 66 2e 0f 1f 84 00 	data16 cs nopw 0x0(%rax,%rax,1)
   14000723c:	00 00 00 00 
   140007240:	41 0f b6 c3          	movzbl %r11b,%eax
   140007244:	4d 8b 1c c2          	mov    (%r10,%rax,8),%r11
   140007248:	49 85 d3             	test   %rdx,%r11
   14000724b:	0f 94 c1             	sete   %cl
   14000724e:	74 23                	je     0x140007273
   140007250:	49 09 d3             	or     %rdx,%r11
   140007253:	49 81 fb bf f2 03 00 	cmp    $0x3f2bf,%r11
   14000725a:	77 17                	ja     0x140007273
   14000725c:	49 c1 eb 04          	shr    $0x4,%r11
   140007260:	47 0f b6 9c 13 94 01 	movzbl 0x194(%r11,%r10,1),%r11d
   140007267:	00 00 
   140007269:	41 80 fb 31          	cmp    $0x31,%r11b
   14000726d:	72 d1                	jb     0x140007240
   14000726f:	eb 02                	jmp    0x140007273
   140007271:	31 c9                	xor    %ecx,%ecx
   140007273:	41 83 f9 01          	cmp    $0x1,%r9d
   140007277:	74 6c                	je     0x1400072e5
   140007279:	45 85 c9             	test   %r9d,%r9d
   14000727c:	0f 85 3b 03 00 00    	jne    0x1400075bd
   140007282:	84 c9                	test   %cl,%cl
   140007284:	0f 84 33 03 00 00    	je     0x1400075bd
   14000728a:	44 89 f9             	mov    %r15d,%ecx
   14000728d:	48 81 f9 00 f0 05 00 	cmp    $0x5f000,%rcx
   140007294:	0f 82 23 03 00 00    	jb     0x1400075bd
   14000729a:	48 3b 0f             	cmp    (%rdi),%rcx
   14000729d:	0f 83 1a 03 00 00    	jae    0x1400075bd
   1400072a3:	41 f6 c7 0f          	test   $0xf,%r15b
   1400072a7:	0f 85 10 03 00 00    	jne    0x1400075bd
   1400072ad:	44 89 fa             	mov    %r15d,%edx
   1400072b0:	c1 ea 04             	shr    $0x4,%edx
   1400072b3:	44 8d 48 01          	lea    0x1(%rax),%r9d
   1400072b7:	44 38 8c 17 88 15 00 	cmp    %r9b,0x1588(%rdi,%rdx,1)
   1400072be:	00 
   1400072bf:	0f 85 f8 02 00 00    	jne    0x1400075bd
   1400072c5:	48 83 f8 30          	cmp    $0x30,%rax
   1400072c9:	0f 86 27 02 00 00    	jbe    0x1400074f6
   1400072cf:	48 c7 04 0f 00 00 00 	movq   $0x0,(%rdi,%rcx,1)
   1400072d6:	00 
   1400072d7:	41 80 c9 80          	or     $0x80,%r9b
   1400072db:	44 88 8c 17 88 15 00 	mov    %r9b,0x1588(%rdi,%rdx,1)
   1400072e2:	00 
   1400072e3:	eb 65                	jmp    0x14000734a
   1400072e5:	84 c9                	test   %cl,%cl
   1400072e7:	0f 84 d0 02 00 00    	je     0x1400075bd
   1400072ed:	44 89 f9             	mov    %r15d,%ecx
   1400072f0:	48 81 f9 00 20 04 00 	cmp    $0x42000,%rcx
   1400072f7:	0f 82 c0 02 00 00    	jb     0x1400075bd
   1400072fd:	48 3b 0f             	cmp    (%rdi),%rcx
   140007300:	0f 83 b7 02 00 00    	jae    0x1400075bd
   140007306:	41 f6 c7 0f          	test   $0xf,%r15b
   14000730a:	0f 85 ad 02 00 00    	jne    0x1400075bd
   140007310:	44 89 fa             	mov    %r15d,%edx
   140007313:	c1 ea 04             	shr    $0x4,%edx
   140007316:	44 0f b6 8c 17 88 15 	movzbl 0x1588(%rdi,%rdx,1),%r9d
   14000731d:	00 00 
   14000731f:	44 8d 50 01          	lea    0x1(%rax),%r10d
   140007323:	45 38 d1             	cmp    %r10b,%r9b
   140007326:	0f 85 91 02 00 00    	jne    0x1400075bd
   14000732c:	48 83 f8 30          	cmp    $0x30,%rax
   140007330:	0f 86 f9 01 00 00    	jbe    0x14000752f
   140007336:	48 c7 04 0f 00 00 00 	movq   $0x0,(%rdi,%rcx,1)
   14000733d:	00 
   14000733e:	41 80 c9 80          	or     $0x80,%r9b
   140007342:	44 88 8c 17 88 15 00 	mov    %r9b,0x1588(%rdi,%rdx,1)
   140007349:	00 
   14000734a:	8b 4f 14             	mov    0x14(%rdi),%ecx
   14000734d:	85 c9                	test   %ecx,%ecx
   14000734f:	0f 84 83 02 00 00    	je     0x1400075d8
   140007355:	4c 89 c3             	mov    %r8,%rbx
   140007358:	ff c9                	dec    %ecx
   14000735a:	89 4f 14             	mov    %ecx,0x14(%rdi)
   14000735d:	e9 16 02 00 00       	jmp    0x140007578
   140007362:	4c 89 c6             	mov    %r8,%rsi
   140007365:	e8 26 31 03 00       	call   0x14003a490
   14000736a:	49 89 f0             	mov    %rsi,%r8
   14000736d:	e9 4b 02 00 00       	jmp    0x1400075bd
   140007372:	48 8b 47 30          	mov    0x30(%rdi),%rax
   140007376:	48 8b 4f 08          	mov    0x8(%rdi),%rcx
   14000737a:	48 01 f8             	add    %rdi,%rax
   14000737d:	0f 82 3a 02 00 00    	jb     0x1400075bd
   140007383:	48 39 d8             	cmp    %rbx,%rax
   140007386:	0f 85 31 02 00 00    	jne    0x1400075bd
   14000738c:	48 89 7c 24 20       	mov    %rdi,0x20(%rsp)
   140007391:	48 89 f1             	mov    %rsi,%rcx
   140007394:	48 89 d0             	mov    %rdx,%rax
   140007397:	48 89 da             	mov    %rbx,%rdx
   14000739a:	4c 89 c6             	mov    %r8,%rsi
   14000739d:	49 89 c0             	mov    %rax,%r8
   1400073a0:	49 89 f1             	mov    %rsi,%r9
   1400073a3:	e8 f8 59 03 00       	call   0x14003cda0
   1400073a8:	eb c0                	jmp    0x14000736a
   1400073aa:	41 b9 00 f0 05 00    	mov    $0x5f000,%r9d
   1400073b0:	44 89 fa             	mov    %r15d,%edx
   1400073b3:	4c 39 ca             	cmp    %r9,%rdx
   1400073b6:	0f 82 01 02 00 00    	jb     0x1400075bd
   1400073bc:	41 f6 c7 0f          	test   $0xf,%r15b
   1400073c0:	0f 85 f7 01 00 00    	jne    0x1400075bd
   1400073c6:	41 c1 ef 04          	shr    $0x4,%r15d
   1400073ca:	42 0f b6 ac 3f 88 15 	movzbl 0x1588(%rdi,%r15,1),%ebp
   1400073d1:	00 00 
   1400073d3:	45 8d 4e 01          	lea    0x1(%r14),%r9d
   1400073d7:	44 38 cd             	cmp    %r9b,%bpl
   1400073da:	0f 85 dd 01 00 00    	jne    0x1400075bd
   1400073e0:	48 3b 17             	cmp    (%rdi),%rdx
   1400073e3:	0f 83 d4 01 00 00    	jae    0x1400075bd
   1400073e9:	4c 8d 24 0e          	lea    (%rsi,%rcx,1),%r12
   1400073ed:	49 81 c4 a8 04 00 00 	add    $0x4a8,%r12
   1400073f4:	44 0f b6 e8          	movzbl %al,%r13d
   1400073f8:	48 8d 0d 86 db 03 00 	lea    0x3db86(%rip),%rcx        # 0x140044f85
   1400073ff:	45 3a 2c 0e          	cmp    (%r14,%rcx,1),%r13b
   140007403:	73 20                	jae    0x140007425
   140007405:	40 80 cd 40          	or     $0x40,%bpl
   140007409:	42 88 ac 3f 88 15 00 	mov    %bpl,0x1588(%rdi,%r15,1)
   140007410:	00 
   140007411:	41 83 e5 0f          	and    $0xf,%r13d
   140007415:	4b 89 5c ec 08       	mov    %rbx,0x8(%r12,%r13,8)
   14000741a:	fe c0                	inc    %al
   14000741c:	41 88 04 24          	mov    %al,(%r12)
   140007420:	e9 98 01 00 00       	jmp    0x1400075bd
   140007425:	41 8d 46 e6          	lea    -0x1a(%r14),%eax
   140007429:	83 f8 17             	cmp    $0x17,%eax
   14000742c:	73 4c                	jae    0x14000747a
   14000742e:	4c 89 44 24 30       	mov    %r8,0x30(%rsp)
   140007433:	49 8d 4d ff          	lea    -0x1(%r13),%rcx
   140007437:	48 83 f9 10          	cmp    $0x10,%rcx
   14000743b:	0f 83 c0 01 00 00    	jae    0x140007601
   140007441:	4b 8b 04 ec          	mov    (%r12,%r13,8),%rax
   140007445:	48 89 44 24 28       	mov    %rax,0x28(%rsp)
   14000744a:	4c 8d 44 24 28       	lea    0x28(%rsp),%r8
   14000744f:	41 b9 01 00 00 00    	mov    $0x1,%r9d
   140007455:	48 89 f1             	mov    %rsi,%rcx
   140007458:	4c 89 f2             	mov    %r14,%rdx
   14000745b:	e8 80 20 00 00       	call   0x1400094e0
   140007460:	4b 89 1c ec          	mov    %rbx,(%r12,%r13,8)
   140007464:	40 80 cd 40          	or     $0x40,%bpl
   140007468:	42 88 ac 3f 88 15 00 	mov    %bpl,0x1588(%rdi,%r15,1)
   14000746f:	00 
   140007470:	4c 8b 44 24 30       	mov    0x30(%rsp),%r8
   140007475:	e9 43 01 00 00       	jmp    0x1400075bd
   14000747a:	49 8d 44 24 08       	lea    0x8(%r12),%rax
   14000747f:	41 b9 08 00 00 00    	mov    $0x8,%r9d
   140007485:	48 89 f1             	mov    %rsi,%rcx
   140007488:	4c 89 f2             	mov    %r14,%rdx
   14000748b:	4d 89 c5             	mov    %r8,%r13
   14000748e:	49 89 c0             	mov    %rax,%r8
   140007491:	e8 4a 20 00 00       	call   0x1400094e0
   140007496:	4d 89 e8             	mov    %r13,%r8
   140007499:	44 89 f0             	mov    %r14d,%eax
   14000749c:	c1 e0 07             	shl    $0x7,%eax
   14000749f:	4a 8d 04 f0          	lea    (%rax,%r14,8),%rax
   1400074a3:	0f 10 84 06 f0 04 00 	movups 0x4f0(%rsi,%rax,1),%xmm0
   1400074aa:	00 
   1400074ab:	0f 10 8c 06 00 05 00 	movups 0x500(%rsi,%rax,1),%xmm1
   1400074b2:	00 
   1400074b3:	0f 10 94 06 10 05 00 	movups 0x510(%rsi,%rax,1),%xmm2
   1400074ba:	00 
   1400074bb:	0f 10 9c 06 20 05 00 	movups 0x520(%rsi,%rax,1),%xmm3
   1400074c2:	00 
   1400074c3:	41 0f 11 5c 24 38    	movups %xmm3,0x38(%r12)
   1400074c9:	41 0f 11 54 24 28    	movups %xmm2,0x28(%r12)
   1400074cf:	41 0f 11 4c 24 18    	movups %xmm1,0x18(%r12)
   1400074d5:	41 0f 11 44 24 08    	movups %xmm0,0x8(%r12)
   1400074db:	40 80 cd 40          	or     $0x40,%bpl
   1400074df:	42 88 ac 3f 88 15 00 	mov    %bpl,0x1588(%rdi,%r15,1)
   1400074e6:	00 
   1400074e7:	49 89 5c 24 48       	mov    %rbx,0x48(%r12)
   1400074ec:	41 c6 04 24 09       	movb   $0x9,(%r12)
   1400074f1:	e9 c7 00 00 00       	jmp    0x1400075bd
   1400074f6:	4c 89 c3             	mov    %r8,%rbx
   1400074f9:	44 8b 84 87 00 14 00 	mov    0x1400(%rdi,%rax,4),%r8d
   140007500:	00 
   140007501:	4e 8d 14 07          	lea    (%rdi,%r8,1),%r10
   140007505:	45 31 db             	xor    %r11d,%r11d
   140007508:	41 be ff ff ff ff    	mov    $0xffffffff,%r14d
   14000750e:	4d 39 f0             	cmp    %r14,%r8
   140007511:	4d 0f 45 da          	cmovne %r10,%r11
   140007515:	4c 89 1c 0f          	mov    %r11,(%rdi,%rcx,1)
   140007519:	44 89 bc 87 00 14 00 	mov    %r15d,0x1400(%rdi,%rax,4)
   140007520:	00 
   140007521:	41 80 c9 80          	or     $0x80,%r9b
   140007525:	44 88 8c 17 88 15 00 	mov    %r9b,0x1588(%rdi,%rdx,1)
   14000752c:	00 
   14000752d:	eb 37                	jmp    0x140007566
   14000752f:	4c 89 c3             	mov    %r8,%rbx
   140007532:	44 8b 84 87 00 14 00 	mov    0x1400(%rdi,%rax,4),%r8d
   140007539:	00 
   14000753a:	4e 8d 14 07          	lea    (%rdi,%r8,1),%r10
   14000753e:	45 31 db             	xor    %r11d,%r11d
   140007541:	41 be ff ff ff ff    	mov    $0xffffffff,%r14d
   140007547:	4d 39 f0             	cmp    %r14,%r8
   14000754a:	4d 0f 45 da          	cmovne %r10,%r11
   14000754e:	4c 89 1c 0f          	mov    %r11,(%rdi,%rcx,1)
   140007552:	44 89 bc 87 00 14 00 	mov    %r15d,0x1400(%rdi,%rax,4)
   140007559:	00 
   14000755a:	41 80 c9 80          	or     $0x80,%r9b
   14000755e:	44 88 8c 17 88 15 00 	mov    %r9b,0x1588(%rdi,%rdx,1)
   140007565:	00 
   140007566:	8b 4f 14             	mov    0x14(%rdi),%ecx
   140007569:	85 c9                	test   %ecx,%ecx
   14000756b:	74 6b                	je     0x1400075d8
   14000756d:	ff c9                	dec    %ecx
   14000756f:	89 4f 14             	mov    %ecx,0x14(%rdi)
   140007572:	41 83 f8 ff          	cmp    $0xffffffff,%r8d
   140007576:	75 16                	jne    0x14000758e
   140007578:	44 8b 47 70          	mov    0x70(%rdi),%r8d
   14000757c:	48 8b 8e e8 03 00 00 	mov    0x3e8(%rsi),%rcx
   140007583:	48 89 c2             	mov    %rax,%rdx
   140007586:	e8 f5 23 00 00       	call   0x140009980
   14000758b:	8b 4f 14             	mov    0x14(%rdi),%ecx
   14000758e:	85 c9                	test   %ecx,%ecx
   140007590:	49 89 d8             	mov    %rbx,%r8
   140007593:	75 28                	jne    0x1400075bd
   140007595:	48 3b be a8 03 00 00 	cmp    0x3a8(%rsi),%rdi
   14000759c:	74 1f                	je     0x1400075bd
   14000759e:	83 7f 18 00          	cmpl   $0x0,0x18(%rdi)
   1400075a2:	75 19                	jne    0x1400075bd
   1400075a4:	80 7f 1c 01          	cmpb   $0x1,0x1c(%rdi)
   1400075a8:	75 13                	jne    0x1400075bd
   1400075aa:	48 89 f1             	mov    %rsi,%rcx
   1400075ad:	48 89 fa             	mov    %rdi,%rdx
   1400075b0:	4c 89 c6             	mov    %r8,%rsi
   1400075b3:	e8 38 24 00 00       	call   0x1400099f0
   1400075b8:	e9 ad fd ff ff       	jmp    0x14000736a
   1400075bd:	4c 89 44 24 28       	mov    %r8,0x28(%rsp)
   1400075c2:	48 8d 44 24 28       	lea    0x28(%rsp),%rax
   1400075c7:	48 83 c4 38          	add    $0x38,%rsp
   1400075cb:	5b                   	pop    %rbx
   1400075cc:	5d                   	pop    %rbp
   1400075cd:	5f                   	pop    %rdi
   1400075ce:	5e                   	pop    %rsi
   1400075cf:	41 5c                	pop    %r12
   1400075d1:	41 5d                	pop    %r13
   1400075d3:	41 5e                	pop    %r14
   1400075d5:	41 5f                	pop    %r15
   1400075d7:	c3                   	ret
   1400075d8:	48 8d 0d c9 8b 03 00 	lea    0x38bc9(%rip),%rcx        # 0x1400401a8
   1400075df:	ba 3a 00 00 00       	mov    $0x3a,%edx
   1400075e4:	e8 c7 53 03 00       	call   0x14003c9b0
   1400075e9:	0f 0b                	ud2
   1400075eb:	4c 8d 05 96 8c 03 00 	lea    0x38c96(%rip),%r8        # 0x140040288
   1400075f2:	ba 31 00 00 00       	mov    $0x31,%edx
   1400075f7:	4c 89 f1             	mov    %r14,%rcx
   1400075fa:	e8 d4 26 03 00       	call   0x140039cd3
   1400075ff:	0f 0b                	ud2
   140007601:	4c 8d 05 98 8c 03 00 	lea    0x38c98(%rip),%r8        # 0x1400402a0
   140007608:	ba 10 00 00 00       	mov    $0x10,%edx
   14000760d:	e8 c1 26 03 00       	call   0x140039cd3
   140007612:	0f 0b                	ud2
