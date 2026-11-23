local L0_1, L1_1, L2_1
inherit("MrxSupport", false)
import("MrxSupportDesignatorSmoke", false)

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2
  L2_2 = {}
  setmetatable(L2_2, A0_2)
  A0_2.__index = A0_2
  L3_2 = MrxSupportDesignatorSmoke
  L3_2 = L3_2.Create(L3_2)
  L3_2.SetSmokeColor(L3_2, "red")
  L3_2.SetAATestLevel(L3_2, "basic")
  L3_2.SetValidationFunction(L3_2, nil)
  L2_2.SetOwner(L2_2, A1_2)
  L2_2.SetRecruit(L2_2, "Pilot")
  L2_2.SetDesignator(L2_2, L3_2)
  L2_2.SetModuleName(L2_2, "MrxTankBuster")
  return L2_2
end

Create = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2
  L1_2 = Pg.FindPointFromCamera
  L2_2 = 400
  L3_2 = 150
  L1_2, L2_2, L3_2 = L1_2(L2_2, L3_2, -1, A0_2.uOwner)
  L4_2 = Pg.FindPointFromCamera
  L5_2 = 200
  L6_2 = 60
  L4_2, L5_2, L6_2 = L4_2(L5_2, L6_2, -1, A0_2.uOwner)
  L16_2 = {}
  L16_2[1] = A0_2
  A0_2.uJet = Airstrike.Flyby(A0_2.uDeliveryVehicle, L1_2, L3_2, L4_2, L6_2, L5_2, 120, Strike, L16_2)
  L9_2 = {}
  L9_2[1] = 3
  L11_2 = {}
  L13_2 = {}
  L13_2[1] = "Misha-None-Freeplay-Support-07"
  L13_2[2] = "Misha-None-Freeplay-Support-11"
  L13_2[3] = "Misha-None-Freeplay-Support-13"
  L13_2[4] = "Misha-None-Freeplay-Support-22"
  L13_2[5] = "Misha-None-Freeplay-Support-33"
  L11_2[1] = A0_2.uJet
  L11_2[2] = L13_2
  Event.Create(Event.TimerRelative, L9_2, MrxSupport.PlayAirstrikeVO, L11_2)
end

DesignationCallback = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2
  L1_2 = Player.GetCharacter(A0_2.uOwner)
  L2_2 = Object.GetPosition
  L3_2 = L1_2
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  L9_2 = 200
  L5_2 = Pg.FastCollectTanks(L2_2, L3_2, L4_2, L9_2)
  L6_2 = 0
  L7_2 = ipairs
  L8_2 = L5_2
  L7_2, L8_2, L9_2 = L7_2(L8_2)
  for L10_2, L11_2 in L7_2, L8_2, L9_2 do
    L12_2 = Vehicle.GetDriver(L11_2)
    L13_2 = Object.IsAlive(L11_2)
    if L13_2 then
      if L12_2 then
        L13_2 = Object.HasLabel(L12_2, "PMC")
        if L13_2 then
          goto lbl_54
        end
      end
      L15_2 = {}
      L15_2[1] = (0.2 * L6_2)
      L17_2 = {}
      L17_2[1] = A0_2
      L17_2[2] = L11_2
      Event.Create(Event.TimerRelative, L15_2, LaunchMissile, L17_2)
      L6_2 = L6_2 + 1
    end
    ::lbl_54::
  end
end

Strike = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2
  L2_2 = Object.GetPosition
  L3_2 = A0_2.uJet
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  L5_2 = Object.GetPosition
  L6_2 = A1_2
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L11_2 = Math.Normalize
  L12_2 = (L5_2 - L2_2)
  L13_2 = (L6_2 - L3_2)
  L11_2, L12_2, L13_2 = L11_2(L12_2, L13_2, (L7_2 - L4_2))
  L8_2 = L11_2
  L11_2 = 30
  L23_2 = A0_2.GetOwner(A0_2)
  L25_2 = {}
  L25_2[1] = A1_2
  L14_2 = Airstrike.SpawnTargettedOrdnance("Airstrike AT Missile", L2_2, L3_2, L4_2, L8_2, L12_2, L13_2, A1_2, "impact", nil, L23_2, Object.Kill, L25_2)
  L15_2 = {}
  L15_2[1] = 255
  L15_2[2] = 0
  L15_2[3] = 0
  BlipAircraft(L14_2, L15_2)
end

LaunchMissile = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2, A4_2)
  local L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L6_2 = {}
  L6_2.Callback = A0_2
  L7_2 = {}
  L7_2[1] = A1_2
  L7_2[2] = A2_2
  L7_2[3] = A3_2
  L6_2.Location = L7_2
  L6_2.InnerRadius = 1
  L6_2.InnerHeightTolerance = 1
  L6_2.OuterRadius = 2
  L6_2.OuterHeightTolerance = 2
  L6_2.HeightMax = 5
  L6_2.SearchRadius = 12
  L6_2.Water = false
  L5_2 = Ai.TestDropZone(L6_2)
  if not L5_2 then
    A0_2(false, "noland")
  end
end

_ValidateDropZone = L0_1
