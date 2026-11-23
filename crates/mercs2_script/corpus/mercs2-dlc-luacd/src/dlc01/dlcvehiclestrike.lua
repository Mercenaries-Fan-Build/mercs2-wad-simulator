local L0_1, L1_1, L2_1
import("MrxPmc", false)
import("MrxTankBuster", false)
import("MrxSupportData", false)
import("MrxGuiInterface", false)
import("MrxSubtitle", false)
import("MrxUtil", false)
import("MrxVoSequence", false)
import("MrxGuiBase", false)
import("MrxTutorialManager", false)
ksTankBuster = "FreeTankBuster"

function L0_1()
  local L0_2, L1_2, L2_2, L3_2
  L0_2 = MrxTankBuster
  L0_2 = L0_2.Create(L0_2)
  L1_2 = MrxSupportData.tFreebieData
  L3_2 = {}
  L3_2.sName = "[support.airstrike.tankbuster.name]"
  L3_2.sIcon = "support_tank_buster"
  L3_2.nFreebieQty = 1
  L3_2.oSupport = L0_2
  L1_2[ksTankBuster] = L3_2
end

CreateTankSupport = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  if not A1_2 then
    A1_2 = {}
  end
  L4_2 = {}
  L4_2.__index = A0_2
  setmetatable(A1_2, L4_2)
  A1_2._tEvents = {}
  L2_2 = A1_2.uPlayer
  if not L2_2 then
    L2_2 = Player.GetLocalPlayer()
  end
  A1_2.uPlayer = L2_2
  L2_2 = A1_2.sStrikeName
  if not L2_2 then
    L2_2 = ksTankBuster
  end
  A1_2.sStrikeName = L2_2
  L4_2 = Player.GetCharacter
  L5_2 = A1_2.uPlayer
  L4_2, L5_2 = L4_2(L5_2)
  A1_2.OnExitVeh(A1_2, L4_2, L5_2)
  return A1_2
end

Create = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = pairs
  L2_2 = A0_2._tEvents
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    Event.Delete(L5_2)
  end
  A0_2._tEvents = nil
  MrxSupportData.RemoveFreebie(A0_2.sStrikeName)
end

Cleanup = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L3_2 = A0_2._tEvents
  L6_2 = {}
  L6_2[1] = A1_2
  L6_2[2] = A2_2
  L6_2[3] = "a"
  L6_2[4] = "x"
  L8_2 = {}
  L8_2[1] = A0_2
  L3_2.eVehicleEnter = Event.Create(Event.ObjectInSeat, L6_2, OnExitVeh, L8_2)
  L3_2 = A0_2._tEvents
  L6_2 = {}
  L6_2[1] = Object.IsPlayerControlled(A1_2)
  L6_2[2] = "lbutton"
  L6_2[3] = "press"
  L6_2[4] = true
  L8_2 = {}
  L8_2[1] = A0_2
  L3_2.eButtonPress = Event.CreatePersistent(Event.Button, L6_2, OnButtonPress, L8_2)
end

OnEnterVeh = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L3_2 = A0_2._tEvents
  L6_2 = {}
  L6_2[1] = A1_2
  L6_2[2] = "Vehicle"
  L6_2[3] = "a"
  L6_2[4] = "e"
  L8_2 = {}
  L8_2[1] = A0_2
  L3_2.eVehicleEnter = Event.Create(Event.ObjectInSeat, L6_2, OnEnterVeh, L8_2)
  Event.Delete(A0_2._tEvents.eButtonPress)
  L3_2 = A0_2._tEvents
  L3_2.eButton = nil
end

OnExitVeh = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L2_2 = Player.GetLocalPlayer
  L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2 = L2_2()
  L1_2 = MrxGuiBase.GetCurrentControlHolder(L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2)
  if L1_2 then
    return
  end
  L3_2 = type(MrxPmc.GetFreebieQty(MrxSupportData.GetFreebie(A0_2.sStrikeName).sName))
  if L3_2 == "number" and 0 < L2_2 then
    L3_2 = L1_2.oSupport
    L3_2 = L3_2.GetDenialCondition(L3_2)
    if L3_2 == nil then
      L3_2 = {}
      L3_2[1] = "Fiona-In-Mission-Contract-Dlc03-08"
      L3_2[2] = "Fiona-In-Mission-Contract-Dlc03-09"
      L3_2[3] = "Fiona-In-Mission-Contract-Dlc03-10"
      L3_2[4] = "Fiona-In-Mission-Contract-Dlc03-11"
      L3_2[5] = "Fiona-In-Mission-Contract-Dlc03-12"
      L4_2 = MrxUtil.GetRandomTableElement(L3_2)
      L5_2 = Hud.MessageBox
      L7_2 = {}
      L7_2.sMessage = "[vehicles.dlcCon003_vehicleStrike]"
      L5_2.AddMessage(L5_2, L7_2)
      L6_2 = {}
      L6_2[1] = L4_2
      MrxVoSequence.Start(L6_2)
      MrxPmc.SetFreebieQty(L1_2.sName, (L2_2 - 1))
      L5_2 = L1_2.oSupport
      L5_2.DesignationCallback(L5_2)
      MrxTutorialManager.HideMessage(true)
    else
    end
  end
end

OnButtonPress = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L2_2 = MrxSupportData.GetFreebie(A0_2.sStrikeName)
  if not A1_2 then
    A1_2 = 1
  end
  L3_2 = MrxPmc.GetFreebieQty(L2_2.sName)
  if not L3_2 then
    L3_2 = 0
  end
  MrxSupportData.RemoveFreebie(A0_2.sStrikeName)
  MrxSupportData.AddFreebie(A0_2.sStrikeName, (A1_2 + L3_2), A0_2.uPlayer)
end

AddStrike = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L2_2 = 1
  L3_2 = A0_2
  L4_2 = 1
  for L5_2 = L2_2, L3_2, L4_2 do
    Pg.SpawnFromCamera("amx30", (A1_2 + (L5_2 * 15)))
  end
end

TestTank = L0_1
