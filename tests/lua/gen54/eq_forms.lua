
local a = ...
return a == 1, 1 == a, a == 1.0, a == 1.5, a == "s", "s" == a, a == nil, nil == a, a == true, a == a, a ~= 1, a ~= "x",
       a == 127, a == 128, a == -127, a == -128, a == 1e100, a == 2^53
