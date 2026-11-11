local M = {}
function M.f1(x)
  local t = { a = x + 1, b = "s1", c = { 1, 2, 3 } }
  if t.a > 10 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f2(x)
  local t = { a = x + 2, b = "s2", c = { 2, 3, 4 } }
  if t.a > 20 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f3(x)
  local t = { a = x + 3, b = "s3", c = { 3, 4, 5 } }
  if t.a > 30 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f4(x)
  local t = { a = x + 4, b = "s4", c = { 4, 5, 6 } }
  if t.a > 40 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f5(x)
  local t = { a = x + 5, b = "s5", c = { 5, 6, 7 } }
  if t.a > 50 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f6(x)
  local t = { a = x + 6, b = "s6", c = { 6, 7, 8 } }
  if t.a > 60 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f7(x)
  local t = { a = x + 7, b = "s7", c = { 7, 8, 9 } }
  if t.a > 70 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f8(x)
  local t = { a = x + 8, b = "s8", c = { 8, 9, 10 } }
  if t.a > 80 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f9(x)
  local t = { a = x + 9, b = "s9", c = { 9, 10, 11 } }
  if t.a > 90 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f10(x)
  local t = { a = x + 10, b = "s10", c = { 10, 11, 12 } }
  if t.a > 100 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f11(x)
  local t = { a = x + 11, b = "s11", c = { 11, 12, 13 } }
  if t.a > 110 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f12(x)
  local t = { a = x + 12, b = "s12", c = { 12, 13, 14 } }
  if t.a > 120 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f13(x)
  local t = { a = x + 13, b = "s13", c = { 13, 14, 15 } }
  if t.a > 130 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f14(x)
  local t = { a = x + 14, b = "s14", c = { 14, 15, 16 } }
  if t.a > 140 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f15(x)
  local t = { a = x + 15, b = "s15", c = { 15, 16, 17 } }
  if t.a > 150 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f16(x)
  local t = { a = x + 16, b = "s16", c = { 16, 17, 18 } }
  if t.a > 160 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f17(x)
  local t = { a = x + 17, b = "s17", c = { 17, 18, 19 } }
  if t.a > 170 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f18(x)
  local t = { a = x + 18, b = "s18", c = { 18, 19, 20 } }
  if t.a > 180 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f19(x)
  local t = { a = x + 19, b = "s19", c = { 19, 20, 21 } }
  if t.a > 190 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f20(x)
  local t = { a = x + 20, b = "s20", c = { 20, 21, 22 } }
  if t.a > 200 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f21(x)
  local t = { a = x + 21, b = "s21", c = { 21, 22, 23 } }
  if t.a > 210 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f22(x)
  local t = { a = x + 22, b = "s22", c = { 22, 23, 24 } }
  if t.a > 220 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f23(x)
  local t = { a = x + 23, b = "s23", c = { 23, 24, 25 } }
  if t.a > 230 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f24(x)
  local t = { a = x + 24, b = "s24", c = { 24, 25, 26 } }
  if t.a > 240 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f25(x)
  local t = { a = x + 25, b = "s25", c = { 25, 26, 27 } }
  if t.a > 250 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f26(x)
  local t = { a = x + 26, b = "s26", c = { 26, 27, 28 } }
  if t.a > 260 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f27(x)
  local t = { a = x + 27, b = "s27", c = { 27, 28, 29 } }
  if t.a > 270 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f28(x)
  local t = { a = x + 28, b = "s28", c = { 28, 29, 30 } }
  if t.a > 280 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f29(x)
  local t = { a = x + 29, b = "s29", c = { 29, 30, 31 } }
  if t.a > 290 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f30(x)
  local t = { a = x + 30, b = "s30", c = { 30, 31, 32 } }
  if t.a > 300 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f31(x)
  local t = { a = x + 31, b = "s31", c = { 31, 32, 33 } }
  if t.a > 310 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f32(x)
  local t = { a = x + 32, b = "s32", c = { 32, 33, 34 } }
  if t.a > 320 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f33(x)
  local t = { a = x + 33, b = "s33", c = { 33, 34, 35 } }
  if t.a > 330 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f34(x)
  local t = { a = x + 34, b = "s34", c = { 34, 35, 36 } }
  if t.a > 340 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f35(x)
  local t = { a = x + 35, b = "s35", c = { 35, 36, 37 } }
  if t.a > 350 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f36(x)
  local t = { a = x + 36, b = "s36", c = { 36, 37, 38 } }
  if t.a > 360 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f37(x)
  local t = { a = x + 37, b = "s37", c = { 37, 38, 39 } }
  if t.a > 370 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f38(x)
  local t = { a = x + 38, b = "s38", c = { 38, 39, 40 } }
  if t.a > 380 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f39(x)
  local t = { a = x + 39, b = "s39", c = { 39, 40, 41 } }
  if t.a > 390 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f40(x)
  local t = { a = x + 40, b = "s40", c = { 40, 41, 42 } }
  if t.a > 400 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f41(x)
  local t = { a = x + 41, b = "s41", c = { 41, 42, 43 } }
  if t.a > 410 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f42(x)
  local t = { a = x + 42, b = "s42", c = { 42, 43, 44 } }
  if t.a > 420 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f43(x)
  local t = { a = x + 43, b = "s43", c = { 43, 44, 45 } }
  if t.a > 430 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f44(x)
  local t = { a = x + 44, b = "s44", c = { 44, 45, 46 } }
  if t.a > 440 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f45(x)
  local t = { a = x + 45, b = "s45", c = { 45, 46, 47 } }
  if t.a > 450 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f46(x)
  local t = { a = x + 46, b = "s46", c = { 46, 47, 48 } }
  if t.a > 460 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f47(x)
  local t = { a = x + 47, b = "s47", c = { 47, 48, 49 } }
  if t.a > 470 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f48(x)
  local t = { a = x + 48, b = "s48", c = { 48, 49, 50 } }
  if t.a > 480 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f49(x)
  local t = { a = x + 49, b = "s49", c = { 49, 50, 51 } }
  if t.a > 490 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f50(x)
  local t = { a = x + 50, b = "s50", c = { 50, 51, 52 } }
  if t.a > 500 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f51(x)
  local t = { a = x + 51, b = "s51", c = { 51, 52, 53 } }
  if t.a > 510 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f52(x)
  local t = { a = x + 52, b = "s52", c = { 52, 53, 54 } }
  if t.a > 520 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f53(x)
  local t = { a = x + 53, b = "s53", c = { 53, 54, 55 } }
  if t.a > 530 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f54(x)
  local t = { a = x + 54, b = "s54", c = { 54, 55, 56 } }
  if t.a > 540 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f55(x)
  local t = { a = x + 55, b = "s55", c = { 55, 56, 57 } }
  if t.a > 550 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f56(x)
  local t = { a = x + 56, b = "s56", c = { 56, 57, 58 } }
  if t.a > 560 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f57(x)
  local t = { a = x + 57, b = "s57", c = { 57, 58, 59 } }
  if t.a > 570 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f58(x)
  local t = { a = x + 58, b = "s58", c = { 58, 59, 60 } }
  if t.a > 580 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f59(x)
  local t = { a = x + 59, b = "s59", c = { 59, 60, 61 } }
  if t.a > 590 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f60(x)
  local t = { a = x + 60, b = "s60", c = { 60, 61, 62 } }
  if t.a > 600 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f61(x)
  local t = { a = x + 61, b = "s61", c = { 61, 62, 63 } }
  if t.a > 610 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f62(x)
  local t = { a = x + 62, b = "s62", c = { 62, 63, 64 } }
  if t.a > 620 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f63(x)
  local t = { a = x + 63, b = "s63", c = { 63, 64, 65 } }
  if t.a > 630 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f64(x)
  local t = { a = x + 64, b = "s64", c = { 64, 65, 66 } }
  if t.a > 640 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f65(x)
  local t = { a = x + 65, b = "s65", c = { 65, 66, 67 } }
  if t.a > 650 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f66(x)
  local t = { a = x + 66, b = "s66", c = { 66, 67, 68 } }
  if t.a > 660 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f67(x)
  local t = { a = x + 67, b = "s67", c = { 67, 68, 69 } }
  if t.a > 670 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f68(x)
  local t = { a = x + 68, b = "s68", c = { 68, 69, 70 } }
  if t.a > 680 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f69(x)
  local t = { a = x + 69, b = "s69", c = { 69, 70, 71 } }
  if t.a > 690 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f70(x)
  local t = { a = x + 70, b = "s70", c = { 70, 71, 72 } }
  if t.a > 700 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f71(x)
  local t = { a = x + 71, b = "s71", c = { 71, 72, 73 } }
  if t.a > 710 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f72(x)
  local t = { a = x + 72, b = "s72", c = { 72, 73, 74 } }
  if t.a > 720 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f73(x)
  local t = { a = x + 73, b = "s73", c = { 73, 74, 75 } }
  if t.a > 730 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f74(x)
  local t = { a = x + 74, b = "s74", c = { 74, 75, 76 } }
  if t.a > 740 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f75(x)
  local t = { a = x + 75, b = "s75", c = { 75, 76, 77 } }
  if t.a > 750 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f76(x)
  local t = { a = x + 76, b = "s76", c = { 76, 77, 78 } }
  if t.a > 760 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f77(x)
  local t = { a = x + 77, b = "s77", c = { 77, 78, 79 } }
  if t.a > 770 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f78(x)
  local t = { a = x + 78, b = "s78", c = { 78, 79, 80 } }
  if t.a > 780 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f79(x)
  local t = { a = x + 79, b = "s79", c = { 79, 80, 81 } }
  if t.a > 790 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f80(x)
  local t = { a = x + 80, b = "s80", c = { 80, 81, 82 } }
  if t.a > 800 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f81(x)
  local t = { a = x + 81, b = "s81", c = { 81, 82, 83 } }
  if t.a > 810 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f82(x)
  local t = { a = x + 82, b = "s82", c = { 82, 83, 84 } }
  if t.a > 820 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f83(x)
  local t = { a = x + 83, b = "s83", c = { 83, 84, 85 } }
  if t.a > 830 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f84(x)
  local t = { a = x + 84, b = "s84", c = { 84, 85, 86 } }
  if t.a > 840 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f85(x)
  local t = { a = x + 85, b = "s85", c = { 85, 86, 87 } }
  if t.a > 850 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f86(x)
  local t = { a = x + 86, b = "s86", c = { 86, 87, 88 } }
  if t.a > 860 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f87(x)
  local t = { a = x + 87, b = "s87", c = { 87, 88, 89 } }
  if t.a > 870 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f88(x)
  local t = { a = x + 88, b = "s88", c = { 88, 89, 90 } }
  if t.a > 880 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f89(x)
  local t = { a = x + 89, b = "s89", c = { 89, 90, 91 } }
  if t.a > 890 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f90(x)
  local t = { a = x + 90, b = "s90", c = { 90, 91, 92 } }
  if t.a > 900 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f91(x)
  local t = { a = x + 91, b = "s91", c = { 91, 92, 93 } }
  if t.a > 910 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f92(x)
  local t = { a = x + 92, b = "s92", c = { 92, 93, 94 } }
  if t.a > 920 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f93(x)
  local t = { a = x + 93, b = "s93", c = { 93, 94, 95 } }
  if t.a > 930 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f94(x)
  local t = { a = x + 94, b = "s94", c = { 94, 95, 96 } }
  if t.a > 940 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f95(x)
  local t = { a = x + 95, b = "s95", c = { 95, 96, 97 } }
  if t.a > 950 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f96(x)
  local t = { a = x + 96, b = "s96", c = { 96, 97, 98 } }
  if t.a > 960 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f97(x)
  local t = { a = x + 97, b = "s97", c = { 97, 98, 99 } }
  if t.a > 970 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f98(x)
  local t = { a = x + 98, b = "s98", c = { 98, 99, 100 } }
  if t.a > 980 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f99(x)
  local t = { a = x + 99, b = "s99", c = { 99, 100, 101 } }
  if t.a > 990 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f100(x)
  local t = { a = x + 100, b = "s100", c = { 100, 101, 102 } }
  if t.a > 1000 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f101(x)
  local t = { a = x + 101, b = "s101", c = { 101, 102, 103 } }
  if t.a > 1010 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f102(x)
  local t = { a = x + 102, b = "s102", c = { 102, 103, 104 } }
  if t.a > 1020 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f103(x)
  local t = { a = x + 103, b = "s103", c = { 103, 104, 105 } }
  if t.a > 1030 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f104(x)
  local t = { a = x + 104, b = "s104", c = { 104, 105, 106 } }
  if t.a > 1040 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f105(x)
  local t = { a = x + 105, b = "s105", c = { 105, 106, 107 } }
  if t.a > 1050 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f106(x)
  local t = { a = x + 106, b = "s106", c = { 106, 107, 108 } }
  if t.a > 1060 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f107(x)
  local t = { a = x + 107, b = "s107", c = { 107, 108, 109 } }
  if t.a > 1070 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f108(x)
  local t = { a = x + 108, b = "s108", c = { 108, 109, 110 } }
  if t.a > 1080 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f109(x)
  local t = { a = x + 109, b = "s109", c = { 109, 110, 111 } }
  if t.a > 1090 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f110(x)
  local t = { a = x + 110, b = "s110", c = { 110, 111, 112 } }
  if t.a > 1100 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f111(x)
  local t = { a = x + 111, b = "s111", c = { 111, 112, 113 } }
  if t.a > 1110 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f112(x)
  local t = { a = x + 112, b = "s112", c = { 112, 113, 114 } }
  if t.a > 1120 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f113(x)
  local t = { a = x + 113, b = "s113", c = { 113, 114, 115 } }
  if t.a > 1130 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f114(x)
  local t = { a = x + 114, b = "s114", c = { 114, 115, 116 } }
  if t.a > 1140 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f115(x)
  local t = { a = x + 115, b = "s115", c = { 115, 116, 117 } }
  if t.a > 1150 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f116(x)
  local t = { a = x + 116, b = "s116", c = { 116, 117, 118 } }
  if t.a > 1160 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f117(x)
  local t = { a = x + 117, b = "s117", c = { 117, 118, 119 } }
  if t.a > 1170 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f118(x)
  local t = { a = x + 118, b = "s118", c = { 118, 119, 120 } }
  if t.a > 1180 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f119(x)
  local t = { a = x + 119, b = "s119", c = { 119, 120, 121 } }
  if t.a > 1190 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f120(x)
  local t = { a = x + 120, b = "s120", c = { 120, 121, 122 } }
  if t.a > 1200 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f121(x)
  local t = { a = x + 121, b = "s121", c = { 121, 122, 123 } }
  if t.a > 1210 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f122(x)
  local t = { a = x + 122, b = "s122", c = { 122, 123, 124 } }
  if t.a > 1220 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f123(x)
  local t = { a = x + 123, b = "s123", c = { 123, 124, 125 } }
  if t.a > 1230 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f124(x)
  local t = { a = x + 124, b = "s124", c = { 124, 125, 126 } }
  if t.a > 1240 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f125(x)
  local t = { a = x + 125, b = "s125", c = { 125, 126, 127 } }
  if t.a > 1250 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f126(x)
  local t = { a = x + 126, b = "s126", c = { 126, 127, 128 } }
  if t.a > 1260 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f127(x)
  local t = { a = x + 127, b = "s127", c = { 127, 128, 129 } }
  if t.a > 1270 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f128(x)
  local t = { a = x + 128, b = "s128", c = { 128, 129, 130 } }
  if t.a > 1280 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f129(x)
  local t = { a = x + 129, b = "s129", c = { 129, 130, 131 } }
  if t.a > 1290 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f130(x)
  local t = { a = x + 130, b = "s130", c = { 130, 131, 132 } }
  if t.a > 1300 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f131(x)
  local t = { a = x + 131, b = "s131", c = { 131, 132, 133 } }
  if t.a > 1310 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f132(x)
  local t = { a = x + 132, b = "s132", c = { 132, 133, 134 } }
  if t.a > 1320 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f133(x)
  local t = { a = x + 133, b = "s133", c = { 133, 134, 135 } }
  if t.a > 1330 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f134(x)
  local t = { a = x + 134, b = "s134", c = { 134, 135, 136 } }
  if t.a > 1340 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f135(x)
  local t = { a = x + 135, b = "s135", c = { 135, 136, 137 } }
  if t.a > 1350 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f136(x)
  local t = { a = x + 136, b = "s136", c = { 136, 137, 138 } }
  if t.a > 1360 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f137(x)
  local t = { a = x + 137, b = "s137", c = { 137, 138, 139 } }
  if t.a > 1370 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f138(x)
  local t = { a = x + 138, b = "s138", c = { 138, 139, 140 } }
  if t.a > 1380 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f139(x)
  local t = { a = x + 139, b = "s139", c = { 139, 140, 141 } }
  if t.a > 1390 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f140(x)
  local t = { a = x + 140, b = "s140", c = { 140, 141, 142 } }
  if t.a > 1400 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f141(x)
  local t = { a = x + 141, b = "s141", c = { 141, 142, 143 } }
  if t.a > 1410 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f142(x)
  local t = { a = x + 142, b = "s142", c = { 142, 143, 144 } }
  if t.a > 1420 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f143(x)
  local t = { a = x + 143, b = "s143", c = { 143, 144, 145 } }
  if t.a > 1430 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f144(x)
  local t = { a = x + 144, b = "s144", c = { 144, 145, 146 } }
  if t.a > 1440 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f145(x)
  local t = { a = x + 145, b = "s145", c = { 145, 146, 147 } }
  if t.a > 1450 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f146(x)
  local t = { a = x + 146, b = "s146", c = { 146, 147, 148 } }
  if t.a > 1460 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f147(x)
  local t = { a = x + 147, b = "s147", c = { 147, 148, 149 } }
  if t.a > 1470 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f148(x)
  local t = { a = x + 148, b = "s148", c = { 148, 149, 150 } }
  if t.a > 1480 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f149(x)
  local t = { a = x + 149, b = "s149", c = { 149, 150, 151 } }
  if t.a > 1490 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f150(x)
  local t = { a = x + 150, b = "s150", c = { 150, 151, 152 } }
  if t.a > 1500 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f151(x)
  local t = { a = x + 151, b = "s151", c = { 151, 152, 153 } }
  if t.a > 1510 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f152(x)
  local t = { a = x + 152, b = "s152", c = { 152, 153, 154 } }
  if t.a > 1520 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f153(x)
  local t = { a = x + 153, b = "s153", c = { 153, 154, 155 } }
  if t.a > 1530 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f154(x)
  local t = { a = x + 154, b = "s154", c = { 154, 155, 156 } }
  if t.a > 1540 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f155(x)
  local t = { a = x + 155, b = "s155", c = { 155, 156, 157 } }
  if t.a > 1550 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f156(x)
  local t = { a = x + 156, b = "s156", c = { 156, 157, 158 } }
  if t.a > 1560 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f157(x)
  local t = { a = x + 157, b = "s157", c = { 157, 158, 159 } }
  if t.a > 1570 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f158(x)
  local t = { a = x + 158, b = "s158", c = { 158, 159, 160 } }
  if t.a > 1580 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f159(x)
  local t = { a = x + 159, b = "s159", c = { 159, 160, 161 } }
  if t.a > 1590 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f160(x)
  local t = { a = x + 160, b = "s160", c = { 160, 161, 162 } }
  if t.a > 1600 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f161(x)
  local t = { a = x + 161, b = "s161", c = { 161, 162, 163 } }
  if t.a > 1610 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f162(x)
  local t = { a = x + 162, b = "s162", c = { 162, 163, 164 } }
  if t.a > 1620 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f163(x)
  local t = { a = x + 163, b = "s163", c = { 163, 164, 165 } }
  if t.a > 1630 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f164(x)
  local t = { a = x + 164, b = "s164", c = { 164, 165, 166 } }
  if t.a > 1640 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f165(x)
  local t = { a = x + 165, b = "s165", c = { 165, 166, 167 } }
  if t.a > 1650 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f166(x)
  local t = { a = x + 166, b = "s166", c = { 166, 167, 168 } }
  if t.a > 1660 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f167(x)
  local t = { a = x + 167, b = "s167", c = { 167, 168, 169 } }
  if t.a > 1670 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f168(x)
  local t = { a = x + 168, b = "s168", c = { 168, 169, 170 } }
  if t.a > 1680 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f169(x)
  local t = { a = x + 169, b = "s169", c = { 169, 170, 171 } }
  if t.a > 1690 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f170(x)
  local t = { a = x + 170, b = "s170", c = { 170, 171, 172 } }
  if t.a > 1700 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f171(x)
  local t = { a = x + 171, b = "s171", c = { 171, 172, 173 } }
  if t.a > 1710 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f172(x)
  local t = { a = x + 172, b = "s172", c = { 172, 173, 174 } }
  if t.a > 1720 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f173(x)
  local t = { a = x + 173, b = "s173", c = { 173, 174, 175 } }
  if t.a > 1730 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f174(x)
  local t = { a = x + 174, b = "s174", c = { 174, 175, 176 } }
  if t.a > 1740 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f175(x)
  local t = { a = x + 175, b = "s175", c = { 175, 176, 177 } }
  if t.a > 1750 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f176(x)
  local t = { a = x + 176, b = "s176", c = { 176, 177, 178 } }
  if t.a > 1760 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f177(x)
  local t = { a = x + 177, b = "s177", c = { 177, 178, 179 } }
  if t.a > 1770 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f178(x)
  local t = { a = x + 178, b = "s178", c = { 178, 179, 180 } }
  if t.a > 1780 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f179(x)
  local t = { a = x + 179, b = "s179", c = { 179, 180, 181 } }
  if t.a > 1790 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f180(x)
  local t = { a = x + 180, b = "s180", c = { 180, 181, 182 } }
  if t.a > 1800 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f181(x)
  local t = { a = x + 181, b = "s181", c = { 181, 182, 183 } }
  if t.a > 1810 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f182(x)
  local t = { a = x + 182, b = "s182", c = { 182, 183, 184 } }
  if t.a > 1820 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f183(x)
  local t = { a = x + 183, b = "s183", c = { 183, 184, 185 } }
  if t.a > 1830 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f184(x)
  local t = { a = x + 184, b = "s184", c = { 184, 185, 186 } }
  if t.a > 1840 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f185(x)
  local t = { a = x + 185, b = "s185", c = { 185, 186, 187 } }
  if t.a > 1850 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f186(x)
  local t = { a = x + 186, b = "s186", c = { 186, 187, 188 } }
  if t.a > 1860 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f187(x)
  local t = { a = x + 187, b = "s187", c = { 187, 188, 189 } }
  if t.a > 1870 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f188(x)
  local t = { a = x + 188, b = "s188", c = { 188, 189, 190 } }
  if t.a > 1880 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f189(x)
  local t = { a = x + 189, b = "s189", c = { 189, 190, 191 } }
  if t.a > 1890 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f190(x)
  local t = { a = x + 190, b = "s190", c = { 190, 191, 192 } }
  if t.a > 1900 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f191(x)
  local t = { a = x + 191, b = "s191", c = { 191, 192, 193 } }
  if t.a > 1910 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f192(x)
  local t = { a = x + 192, b = "s192", c = { 192, 193, 194 } }
  if t.a > 1920 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f193(x)
  local t = { a = x + 193, b = "s193", c = { 193, 194, 195 } }
  if t.a > 1930 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f194(x)
  local t = { a = x + 194, b = "s194", c = { 194, 195, 196 } }
  if t.a > 1940 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f195(x)
  local t = { a = x + 195, b = "s195", c = { 195, 196, 197 } }
  if t.a > 1950 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f196(x)
  local t = { a = x + 196, b = "s196", c = { 196, 197, 198 } }
  if t.a > 1960 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f197(x)
  local t = { a = x + 197, b = "s197", c = { 197, 198, 199 } }
  if t.a > 1970 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f198(x)
  local t = { a = x + 198, b = "s198", c = { 198, 199, 200 } }
  if t.a > 1980 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f199(x)
  local t = { a = x + 199, b = "s199", c = { 199, 200, 201 } }
  if t.a > 1990 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f200(x)
  local t = { a = x + 200, b = "s200", c = { 200, 201, 202 } }
  if t.a > 2000 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f201(x)
  local t = { a = x + 201, b = "s201", c = { 201, 202, 203 } }
  if t.a > 2010 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f202(x)
  local t = { a = x + 202, b = "s202", c = { 202, 203, 204 } }
  if t.a > 2020 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f203(x)
  local t = { a = x + 203, b = "s203", c = { 203, 204, 205 } }
  if t.a > 2030 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f204(x)
  local t = { a = x + 204, b = "s204", c = { 204, 205, 206 } }
  if t.a > 2040 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f205(x)
  local t = { a = x + 205, b = "s205", c = { 205, 206, 207 } }
  if t.a > 2050 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f206(x)
  local t = { a = x + 206, b = "s206", c = { 206, 207, 208 } }
  if t.a > 2060 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f207(x)
  local t = { a = x + 207, b = "s207", c = { 207, 208, 209 } }
  if t.a > 2070 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f208(x)
  local t = { a = x + 208, b = "s208", c = { 208, 209, 210 } }
  if t.a > 2080 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f209(x)
  local t = { a = x + 209, b = "s209", c = { 209, 210, 211 } }
  if t.a > 2090 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f210(x)
  local t = { a = x + 210, b = "s210", c = { 210, 211, 212 } }
  if t.a > 2100 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f211(x)
  local t = { a = x + 211, b = "s211", c = { 211, 212, 213 } }
  if t.a > 2110 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f212(x)
  local t = { a = x + 212, b = "s212", c = { 212, 213, 214 } }
  if t.a > 2120 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f213(x)
  local t = { a = x + 213, b = "s213", c = { 213, 214, 215 } }
  if t.a > 2130 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f214(x)
  local t = { a = x + 214, b = "s214", c = { 214, 215, 216 } }
  if t.a > 2140 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f215(x)
  local t = { a = x + 215, b = "s215", c = { 215, 216, 217 } }
  if t.a > 2150 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f216(x)
  local t = { a = x + 216, b = "s216", c = { 216, 217, 218 } }
  if t.a > 2160 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f217(x)
  local t = { a = x + 217, b = "s217", c = { 217, 218, 219 } }
  if t.a > 2170 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f218(x)
  local t = { a = x + 218, b = "s218", c = { 218, 219, 220 } }
  if t.a > 2180 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f219(x)
  local t = { a = x + 219, b = "s219", c = { 219, 220, 221 } }
  if t.a > 2190 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f220(x)
  local t = { a = x + 220, b = "s220", c = { 220, 221, 222 } }
  if t.a > 2200 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f221(x)
  local t = { a = x + 221, b = "s221", c = { 221, 222, 223 } }
  if t.a > 2210 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f222(x)
  local t = { a = x + 222, b = "s222", c = { 222, 223, 224 } }
  if t.a > 2220 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f223(x)
  local t = { a = x + 223, b = "s223", c = { 223, 224, 225 } }
  if t.a > 2230 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f224(x)
  local t = { a = x + 224, b = "s224", c = { 224, 225, 226 } }
  if t.a > 2240 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f225(x)
  local t = { a = x + 225, b = "s225", c = { 225, 226, 227 } }
  if t.a > 2250 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f226(x)
  local t = { a = x + 226, b = "s226", c = { 226, 227, 228 } }
  if t.a > 2260 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f227(x)
  local t = { a = x + 227, b = "s227", c = { 227, 228, 229 } }
  if t.a > 2270 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f228(x)
  local t = { a = x + 228, b = "s228", c = { 228, 229, 230 } }
  if t.a > 2280 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f229(x)
  local t = { a = x + 229, b = "s229", c = { 229, 230, 231 } }
  if t.a > 2290 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f230(x)
  local t = { a = x + 230, b = "s230", c = { 230, 231, 232 } }
  if t.a > 2300 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f231(x)
  local t = { a = x + 231, b = "s231", c = { 231, 232, 233 } }
  if t.a > 2310 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f232(x)
  local t = { a = x + 232, b = "s232", c = { 232, 233, 234 } }
  if t.a > 2320 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f233(x)
  local t = { a = x + 233, b = "s233", c = { 233, 234, 235 } }
  if t.a > 2330 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f234(x)
  local t = { a = x + 234, b = "s234", c = { 234, 235, 236 } }
  if t.a > 2340 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f235(x)
  local t = { a = x + 235, b = "s235", c = { 235, 236, 237 } }
  if t.a > 2350 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f236(x)
  local t = { a = x + 236, b = "s236", c = { 236, 237, 238 } }
  if t.a > 2360 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f237(x)
  local t = { a = x + 237, b = "s237", c = { 237, 238, 239 } }
  if t.a > 2370 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f238(x)
  local t = { a = x + 238, b = "s238", c = { 238, 239, 240 } }
  if t.a > 2380 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f239(x)
  local t = { a = x + 239, b = "s239", c = { 239, 240, 241 } }
  if t.a > 2390 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f240(x)
  local t = { a = x + 240, b = "s240", c = { 240, 241, 242 } }
  if t.a > 2400 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f241(x)
  local t = { a = x + 241, b = "s241", c = { 241, 242, 243 } }
  if t.a > 2410 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f242(x)
  local t = { a = x + 242, b = "s242", c = { 242, 243, 244 } }
  if t.a > 2420 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f243(x)
  local t = { a = x + 243, b = "s243", c = { 243, 244, 245 } }
  if t.a > 2430 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f244(x)
  local t = { a = x + 244, b = "s244", c = { 244, 245, 246 } }
  if t.a > 2440 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f245(x)
  local t = { a = x + 245, b = "s245", c = { 245, 246, 247 } }
  if t.a > 2450 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f246(x)
  local t = { a = x + 246, b = "s246", c = { 246, 247, 248 } }
  if t.a > 2460 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f247(x)
  local t = { a = x + 247, b = "s247", c = { 247, 248, 249 } }
  if t.a > 2470 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f248(x)
  local t = { a = x + 248, b = "s248", c = { 248, 249, 250 } }
  if t.a > 2480 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f249(x)
  local t = { a = x + 249, b = "s249", c = { 249, 250, 251 } }
  if t.a > 2490 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f250(x)
  local t = { a = x + 250, b = "s250", c = { 250, 251, 252 } }
  if t.a > 2500 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f251(x)
  local t = { a = x + 251, b = "s251", c = { 251, 252, 253 } }
  if t.a > 2510 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f252(x)
  local t = { a = x + 252, b = "s252", c = { 252, 253, 254 } }
  if t.a > 2520 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f253(x)
  local t = { a = x + 253, b = "s253", c = { 253, 254, 255 } }
  if t.a > 2530 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f254(x)
  local t = { a = x + 254, b = "s254", c = { 254, 255, 256 } }
  if t.a > 2540 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f255(x)
  local t = { a = x + 255, b = "s255", c = { 255, 256, 257 } }
  if t.a > 2550 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f256(x)
  local t = { a = x + 256, b = "s256", c = { 256, 257, 258 } }
  if t.a > 2560 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f257(x)
  local t = { a = x + 257, b = "s257", c = { 257, 258, 259 } }
  if t.a > 2570 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f258(x)
  local t = { a = x + 258, b = "s258", c = { 258, 259, 260 } }
  if t.a > 2580 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f259(x)
  local t = { a = x + 259, b = "s259", c = { 259, 260, 261 } }
  if t.a > 2590 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f260(x)
  local t = { a = x + 260, b = "s260", c = { 260, 261, 262 } }
  if t.a > 2600 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f261(x)
  local t = { a = x + 261, b = "s261", c = { 261, 262, 263 } }
  if t.a > 2610 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f262(x)
  local t = { a = x + 262, b = "s262", c = { 262, 263, 264 } }
  if t.a > 2620 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f263(x)
  local t = { a = x + 263, b = "s263", c = { 263, 264, 265 } }
  if t.a > 2630 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f264(x)
  local t = { a = x + 264, b = "s264", c = { 264, 265, 266 } }
  if t.a > 2640 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f265(x)
  local t = { a = x + 265, b = "s265", c = { 265, 266, 267 } }
  if t.a > 2650 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f266(x)
  local t = { a = x + 266, b = "s266", c = { 266, 267, 268 } }
  if t.a > 2660 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f267(x)
  local t = { a = x + 267, b = "s267", c = { 267, 268, 269 } }
  if t.a > 2670 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f268(x)
  local t = { a = x + 268, b = "s268", c = { 268, 269, 270 } }
  if t.a > 2680 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f269(x)
  local t = { a = x + 269, b = "s269", c = { 269, 270, 271 } }
  if t.a > 2690 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f270(x)
  local t = { a = x + 270, b = "s270", c = { 270, 271, 272 } }
  if t.a > 2700 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f271(x)
  local t = { a = x + 271, b = "s271", c = { 271, 272, 273 } }
  if t.a > 2710 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f272(x)
  local t = { a = x + 272, b = "s272", c = { 272, 273, 274 } }
  if t.a > 2720 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f273(x)
  local t = { a = x + 273, b = "s273", c = { 273, 274, 275 } }
  if t.a > 2730 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f274(x)
  local t = { a = x + 274, b = "s274", c = { 274, 275, 276 } }
  if t.a > 2740 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f275(x)
  local t = { a = x + 275, b = "s275", c = { 275, 276, 277 } }
  if t.a > 2750 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f276(x)
  local t = { a = x + 276, b = "s276", c = { 276, 277, 278 } }
  if t.a > 2760 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f277(x)
  local t = { a = x + 277, b = "s277", c = { 277, 278, 279 } }
  if t.a > 2770 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f278(x)
  local t = { a = x + 278, b = "s278", c = { 278, 279, 280 } }
  if t.a > 2780 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f279(x)
  local t = { a = x + 279, b = "s279", c = { 279, 280, 281 } }
  if t.a > 2790 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f280(x)
  local t = { a = x + 280, b = "s280", c = { 280, 281, 282 } }
  if t.a > 2800 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f281(x)
  local t = { a = x + 281, b = "s281", c = { 281, 282, 283 } }
  if t.a > 2810 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f282(x)
  local t = { a = x + 282, b = "s282", c = { 282, 283, 284 } }
  if t.a > 2820 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f283(x)
  local t = { a = x + 283, b = "s283", c = { 283, 284, 285 } }
  if t.a > 2830 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f284(x)
  local t = { a = x + 284, b = "s284", c = { 284, 285, 286 } }
  if t.a > 2840 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f285(x)
  local t = { a = x + 285, b = "s285", c = { 285, 286, 287 } }
  if t.a > 2850 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f286(x)
  local t = { a = x + 286, b = "s286", c = { 286, 287, 288 } }
  if t.a > 2860 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f287(x)
  local t = { a = x + 287, b = "s287", c = { 287, 288, 289 } }
  if t.a > 2870 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f288(x)
  local t = { a = x + 288, b = "s288", c = { 288, 289, 290 } }
  if t.a > 2880 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f289(x)
  local t = { a = x + 289, b = "s289", c = { 289, 290, 291 } }
  if t.a > 2890 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f290(x)
  local t = { a = x + 290, b = "s290", c = { 290, 291, 292 } }
  if t.a > 2900 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f291(x)
  local t = { a = x + 291, b = "s291", c = { 291, 292, 293 } }
  if t.a > 2910 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f292(x)
  local t = { a = x + 292, b = "s292", c = { 292, 293, 294 } }
  if t.a > 2920 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f293(x)
  local t = { a = x + 293, b = "s293", c = { 293, 294, 295 } }
  if t.a > 2930 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f294(x)
  local t = { a = x + 294, b = "s294", c = { 294, 295, 296 } }
  if t.a > 2940 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f295(x)
  local t = { a = x + 295, b = "s295", c = { 295, 296, 297 } }
  if t.a > 2950 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f296(x)
  local t = { a = x + 296, b = "s296", c = { 296, 297, 298 } }
  if t.a > 2960 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f297(x)
  local t = { a = x + 297, b = "s297", c = { 297, 298, 299 } }
  if t.a > 2970 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f298(x)
  local t = { a = x + 298, b = "s298", c = { 298, 299, 300 } }
  if t.a > 2980 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f299(x)
  local t = { a = x + 299, b = "s299", c = { 299, 300, 301 } }
  if t.a > 2990 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
function M.f300(x)
  local t = { a = x + 300, b = "s300", c = { 300, 301, 302 } }
  if t.a > 3000 then return t.a * 2 end
  for k = 1, 3 do t.a = t.a + t.c[k] end
  return t.a
end
return M
