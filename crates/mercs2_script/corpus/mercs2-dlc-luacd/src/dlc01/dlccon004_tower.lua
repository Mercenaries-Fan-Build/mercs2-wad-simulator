local L0_1, L1_1, L2_1, L3_1
inherit("Blippable", false)
L0_1 = {}
L0_1[1] = 100
L0_1[2] = 75
L0_1[3] = 255
tColor = L0_1
sTexture = "MiniMap_Icon_Clock_DLC"
nSize = 8

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L5_2 = {}
  L5_2[1] = A0_2
  L5_2[2] = "awake"
  L7_2 = {}
  L7_2[1] = A0_2
  L7_2[2] = A1_2
  L7_2[3] = A2_2
  Event.Create(Event.ObjectHibernation, L5_2, Start, L7_2)
end

OnActivate = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L4_2 = type(Object.GetHealth(A0_2))
  if L4_2 == "number" and 0 < L3_2 then
    L4_2 = getfenv()
    L5_2 = L4_2.Create(L4_2, A0_2, A1_2)
  end
end

Start = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L1_2 = Object.GetPosition
  L2_2 = A0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  Pg.Spawn("DLCCon004_Timer_Pickup", (L1_2 + math.randf(-2, 2)), (L2_2 + 4), (L3_2 + math.randf(-2, 2)))
  OnDeactivate(A0_2)
end

OnDeath = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2
  L3_2 = Blippable.Create(A0_2, A1_2, A2_2)
  L3_2.SetBlipped(L3_2, A1_2)
  return L3_2
end

Create = L0_1
