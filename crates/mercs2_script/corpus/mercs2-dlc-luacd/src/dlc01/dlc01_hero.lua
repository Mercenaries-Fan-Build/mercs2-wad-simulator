local L0_1, L1_1, L2_1, L3_1, L4_1, L5_1, L6_1, L7_1, L8_1, L9_1, L10_1, L11_1, L12_1, L13_1, L14_1, L15_1
import("MrxGui", false)
import("MrxSound", false)
import("MrxTutorialManager", false)
import("MrxUtil", false)
import("MrxVoSequence", false)
import("MrxPmc", false)
import("WifEquipmentData", false)
import("MrxParkingLotManager", false)
tEvent = {}
L0_1 = 7.5
L1_1 = 4
L2_1 = 0
L3_1 = 0.03
L4_1 = 2
L5_1 = 2.5
L6_1 = 0.3
L7_1 = {}
L8_1 = {}
L9_1 = 10
L10_1 = 0
L11_1 = 3
L12_1 = 100
L13_1 = 1
L14_1 = 0.45

function L15_1()
  local L0_2, L1_2
  MrxParkingLotManager.Setup()
end

Init = L15_1

function L15_1()
  local L0_2, L1_2
  MrxParkingLotManager.Cleanup()
end

Deinit = L15_1

function L15_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L3_2 = {}
  L3_2[1] = A0_2
  L3_2[2] = "awake"
  L5_2 = {}
  L5_2[1] = A0_2
  Event.Create(Event.ObjectHibernation, L3_2, Activate, L5_2)
end

OnActivate = L15_1

function L15_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  L1_2 = Object.IsPlayerControlled(A0_2)
  if L1_2 then
    L2_2 = Player.IsLocal(L1_2)
    if L2_2 then
      L2_2 = GetAttribute(A0_2, "attitude")
      if 2 < L2_2 then
        L2_1 = (L3_1 * 1.5)
        L10_1 = (L11_1 * 1.5)
      else
        L2_1 = L3_1
        L10_1 = L11_1
      end
      L2_2 = tEvent
      L3_2 = tEvent[A0_2]
      if not L3_2 then
        L3_2 = {}
      end
      L2_2[A0_2] = L3_2
      SetupSurvivalSystem(L1_2, A0_2, true)
      SetupInventory(A0_2)
      SetupTransferSystem(L1_2, A0_2)
    else
    end
  end
  L2_2 = Event.Create
  L3_2 = Event.TimerRelative
  L4_2 = {}
  L4_2[1] = 3
  
  function L5_2()
    local L0_3, L1_3, L2_3
    Gui.SetPickupMarkerVisibleDistance(20, false)
    Gui.SetPickupMarkerSize(18, false)
    Gui.SetPickupMarkerSize(18, true)
  end
  
  L2_2(L3_2, L4_2, L5_2)
end

Activate = L15_1

function L15_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L1_2 = Object.IsPlayerControlled(A0_2)
  if L1_2 then
    L2_2 = Player.IsLocal(L1_2)
    if not L2_2 then
      return
    end
  else
    return
  end
  CleanupSurvival(A0_2)
  MrxSound.EndSurvivalMode()
  L2_2 = tEvent[A0_2]
  L5_2 = {}
  L5_2[1] = A0_2
  L5_2[2] = ">"
  L5_2[3] = 5
  L7_2 = {}
  L7_2[1] = L1_2
  L7_2[2] = A0_2
  L7_2[3] = true
  L2_2.FullHealth = Event.Create(Event.ObjectHealth, L5_2, SetupSurvivalSystem, L7_2)
end

OnDeath = L15_1

function L15_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L1_2 = Object.IsPlayerControlled(A0_2)
  if not L1_2 then
    return
  end
  L2_2 = Player.IsLocal(L1_2)
  if not L2_2 then
    return
  end
  SaveOutInventory(A0_2)
  CleanupSurvival(A0_2)
  CleanupTransferSystem(L1_2, A0_2)
end

OnDeactivate = L15_1

function L15_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L1_2 = Human.Inventory.GetAllWeapons(A0_2)
  L2_2 = pairs
  L3_2 = L1_2
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    L7_2 = Weapon.GetMaxReserveAmmo(L6_2)
    if L7_2 then
      Weapon.SetReserveAmmo(L6_2, L7_2)
    end
  end
end

SetupInventory = L15_1

function L15_1(A0_2)
  local L1_2, L2_2
  L1_2 = Human.Inventory.GetAllWeapons(A0_2)
end

SaveOutInventory = L15_1

function L15_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  Event.Delete(tEvent[A0_2].Timer)
  L1_2 = tEvent[A0_2]
  L1_2.Timer = nil
  L1_2 = Object.IsPlayerControlled(A0_2)
  L2_2 = Object.GetHealth(A0_2)
  L3_2 = L7_1[L1_2]
  if L3_2 then
    CreateHealTimer(A0_2, L1_1)
  else
    L3_2 = L9_1
    if L2_2 < L3_2 then
      L3_2 = tEvent[A0_2].Cooldown
      if not L3_2 then
        StartSurvivalMode(L1_2, A0_2)
    end
    else
      L3_2 = tEvent[A0_2].Cooldown
      if L3_2 then
      else
        CreateHealTimer(A0_2, L0_1)
      end
    end
  end
  CreateDropEvent(A0_2)
end

HealthDropped = L15_1

function L15_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L1_2 = Object.GetHealth(A0_2)
  L2_2 = Object.GetMaxHealth(A0_2)
  if not L1_2 then
    return
  end
  L3_2 = (L2_2 - L1_2) * L2_1
  L4_2 = Object.InSeat(A0_2)
  if L4_2 then
    L3_2 = L3_2 * L4_1
  end
  L4_2 = Math.max((L5_1 / L3_2), L6_1)
  L3_2 = Math.max(L3_2, 1)
  L7_2 = Math.min
  L8_2 = L1_2 + L3_2
  L9_2 = L2_2
  L7_2, L8_2, L9_2 = L7_2(L8_2, L9_2)
  Object.SetHealth(A0_2, L7_2, L8_2, L9_2)
  if L1_2 < L2_2 then
    CreateHealTimer(A0_2, L4_2)
  end
  L6_2 = L7_1[Object.IsPlayerControlled(A0_2)]
  if L6_2 then
    L6_2 = L9_1
    if L1_2 >= L6_2 then
      EndSurvivalMode(L5_2, A0_2)
    end
  end
  CreateDropEvent(A0_2)
end

Heal = L15_1

function L15_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  Event.Delete(tEvent[A0_2].Timer)
  L2_2 = tEvent[A0_2]
  L2_2.Timer = nil
  L2_2 = tEvent[A0_2]
  L5_2 = {}
  L5_2[1] = A1_2
  L7_2 = {}
  L7_2[1] = A0_2
  L2_2.Timer = Event.Create(Event.TimerRelative, L5_2, Heal, L7_2)
  L2_2 = Object.IsPlayerControlled(A0_2)
end

CreateHealTimer = L15_1

function L15_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  Event.Delete(tEvent[A0_2].HealthDropped)
  L1_2 = tEvent[A0_2]
  L1_2.HealthDropped = nil
  L1_2 = tEvent[A0_2]
  L4_2 = {}
  L7_2 = Object.GetHealth
  L8_2 = A0_2
  L7_2, L8_2 = L7_2(L8_2)
  L4_2[1] = A0_2
  L4_2[2] = "<"
  L4_2[3] = L7_2
  L4_2[4] = L8_2
  L6_2 = {}
  L6_2[1] = A0_2
  L1_2.HealthDropped = Event.CreatePersistent(Event.ObjectHealth, L4_2, HealthDropped, L6_2)
end

CreateDropEvent = L15_1

function L15_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L1_2 = Object.IsPlayerControlled(A0_2)
  if not L1_2 then
    CleanEvents(tEvent[A0_2])
    return
  end
  L2_2 = tEvent[A0_2]
  if L2_2 then
    L2_2 = tEvent[A0_2].Cooldown
    if L2_2 then
      SurvivalCooldownEnded(L1_2, A0_2)
    end
  end
  MrxGui.FadeFromColor(0, L1_2)
  CleanEvents(tEvent[A0_2])
end

CleanupSurvival = L15_1

function L15_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = type(A0_2)
  if L1_2 ~= "table" then
    return
  end
  L1_2 = pairs
  L2_2 = A0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = type(L5_2)
    if L6_2 == "table" then
      CleanEvents(L5_2)
    else
      Event.Delete(L5_2)
    end
  end
end

CleanEvents = L15_1

function L15_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2
  L3_2 = L7_1
  L3_2[A0_2] = false
  Player.SetSurvivalMode(A0_2, false)
  Player.SetHealthClamp(A0_2, true)
  if A2_2 then
    HealthDropped(A1_2)
  end
end

SetupSurvivalSystem = L15_1

function L15_1(A0_2, A1_2, A2_2)
end

SurvivalModeCallback = L15_1

function L15_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2
  SetupSurvivalSystem(A0_2, A1_2)
  L3_2 = type(A2_2)
  if "number" ~= L3_2 then
    A2_2 = 2
  end
  MrxGui.FadeFromColor(A2_2, A0_2)
  MrxSound.EndSurvivalMode()
end

EndSurvivalMode = L15_1

function L15_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L3_2 = " --> Called from HealthDropped"
  if A2_2 then
    L3_2 = " --> Callback function"
  end
  L4_2 = tEvent[A1_2]
  if L4_2 then
    L4_2 = tEvent[A1_2].Cooldown
    if L4_2 then
      goto lbl_18
    end
  end
  L4_2 = L7_1[A0_2]
  ::lbl_18::
  if L4_2 then
    return
  end
  L6_2 = {}
  L6_2[1] = A1_2
  Event.Post("SurvivalMode", L6_2)
  if not A1_2 then
    A1_2 = Player.GetCharacter(A0_2)
  end
  MrxGui.FadeToColor(0.3, A0_2, 255, 0, 0, L12_1)
  Object.SetInvincible(A1_2, true, "Survival")
  L4_2 = tEvent[A1_2]
  L7_2 = {}
  L7_2[1] = L10_1
  L9_2 = {}
  L9_2[1] = A0_2
  L9_2[2] = A1_2
  L4_2.Cooldown = Event.CreatePersistent(Event.TimerRelative, L7_2, SurvivalCooldownEnded, L9_2)
  MrxSound.BeginSurvivalMode()
end

StartSurvivalMode = L15_1

function L15_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  Event.Delete(tEvent[A1_2].Cooldown)
  L2_2 = tEvent[A1_2]
  L2_2.Cooldown = nil
  L2_2 = L7_1
  L2_2[A0_2] = true
  Object.SetInvincible(A1_2, false, "Survival")
  Player.SetSurvivalMode(A0_2, true)
  Player.SetHealthClamp(A0_2, false)
  MrxGui.FadeFromColor(3, A0_2)
  L4_2 = {}
  L4_2[1] = A1_2
  Event.Post("SurvivalCooldownEnded", L4_2)
  CreateHealTimer(A1_2, L1_1)
  L2_2 = tEvent[A1_2]
  L2_2.nTimeScale = L14_1
end

SurvivalCooldownEnded = L15_1

function L15_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  do return end
  L3_2 = ((Object.GetHealth(A1_2) / L9_1) * (1 - L14_1)) + L14_1
  L5_2 = Math.min
  L6_2 = L3_2
  L7_2 = L13_1
  L5_2, L6_2, L7_2 = L5_2(L6_2, L7_2)
  Sys.SetTimeScale(L5_2, L6_2, L7_2)
end

SetTimeScale = L15_1

function L15_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = nRiders
  if not L1_2 then
    L1_2 = 0
  end
  nRiders = L1_2
  nRiders = (nRiders + 1)
  L1_2 = uRiderEvent
  if L1_2 then
    return
  end
  L3_2 = {}
  L3_2[1] = Player.GetLocalCharacter()
  L3_2[2] = 0
  L3_2[3] = "pg"
  L3_2[4] = "e"
  uRiderEvent = Event.CreatePersistent(Event.ObjectInSeat, L3_2, EnterPassengerCallback)
end

SetupTransferSystem = L15_1

function L15_1(A0_2)
  local L1_2, L2_2
  L1_2 = nRiders
  if L1_2 then
    L1_2 = uRiderEvent
    if L1_2 then
      goto lbl_8
    end
  end
  do return end
  ::lbl_8::
  nRiders = (nRiders - 1)
  L1_2 = nRiders
  if L1_2 == 0 then
    L1_2 = uRiderEvent
    if L1_2 then
      Event.Delete(uRiderEvent)
      uRiderEvent = nil
      nRiders = nil
    end
  end
end

CleanupTransferSystem = L15_1

function L15_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2
  L4_2 = Vehicle.GetDriver(A1_2)
  if L4_2 == A0_2 then
    return
  end
  L5_2 = Vehicle.IsSeatALadder(A3_2)
  if L5_2 then
    return
  end
  if L4_2 then
    L5_2 = Object.IsPlayerControlled(L4_2)
    if L5_2 then
      return
    end
    L5_2 = Object.HasLabel(A1_2, "Car")
    if not L5_2 then
      L5_2 = Object.HasLabel(A1_2, "Truck")
      if not L5_2 then
        return
      end
    end
    if A2_2 == "gunner" then
      L5_2 = Ai.GetRelation(A0_2, L4_2)
      if 0 <= L5_2 then
        return
      end
    end
  elseif A2_2 == "gunner" then
    return
  end
  L5_2 = Vehicle.GetSeatByType(A1_2, "d")
  if L5_2 then
    L6_2 = Vehicle.IsSeatBlocked(L5_2)
    if not L6_2 then
      goto lbl_67
    end
  end
  do return end
  ::lbl_67::
  L7_2 = Vehicle.GetSeatToSeat(Vehicle.GetSeatFromRider(A0_2), false)
  L8_2 = false
  if L7_2 then
    L9_2 = ipairs
    L10_2 = L7_2
    L9_2, L10_2, L11_2 = L9_2(L10_2)
    for L12_2, L13_2 in L9_2, L10_2, L11_2 do
      if L13_2 == L5_2 then
        L8_2 = true
        break
      end
    end
  end
  if not L8_2 then
    return
  end
  if L4_2 then
    Vehicle.Exit(A1_2, L4_2, false)
    L11_2 = {}
    L11_2[1] = L4_2
    L11_2[2] = 0
    L11_2[3] = "a"
    L11_2[4] = "x"
    L13_2 = {}
    L13_2[1] = A0_2
    L13_2[2] = L5_2
    L13_2[3] = false
    Event.Create(Event.ObjectInSeat, L11_2, Vehicle.TransferToSeat, L13_2)
  else
    Vehicle.TransferToSeat(A0_2, L5_2, false)
  end
end

EnterPassengerCallback = L15_1

function L15_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  if not A0_2 or not A1_2 then
    return
  end
  A0_2 = Player.GetCharacter(A0_2) or A0_2
  if not L2_2 then
  end
  L2_2 = Object.IsPlayerControlled(A0_2)
  if not L2_2 then
    return
  end
  L2_2 = nil
  L3_2 = 1
  L4_2 = 3
  L5_2 = 1
  for L6_2 = L3_2, L4_2, L5_2 do
    L9_2 = tostring
    L11_2 = L6_2
    L10_2 = A1_2 .. L11_2
    L9_2, L10_2, L11_2 = L9_2(L10_2)
    L7_2 = Object.HasLabel(A0_2, L9_2, L10_2, L11_2)
    if L7_2 then
      L2_2 = L6_2
    end
    L6_2 = L6_2 + 1
  end
  if not L2_2 then
    L2_2 = 2
  end
  return L2_2
end

GetAttribute = L15_1
_uHideMessage = nil

function L15_1()
  local L0_2, L1_2
  L0_2 = type(_uHideMessage)
  if L0_2 == "userdata" then
    MrxTutorialManager.HideMessage(true)
    Event.Delete(_uHideMessage)
  end
  _uHideMessage = nil
end

HideTutorialMessage = L15_1

function L15_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2
  L2_2 = {}
  L2_2[1] = 5
  _uHideMessage = Event.Create(Event.TimerRelative, L2_2, HideTutorialMessage, {})
end

TutorialCueCallback = L15_1

function L15_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = _uHideMessage
  if not L1_2 then
    L1_2 = MrxPmc.HasEquipment("GrapplingHook")
    if not L1_2 then
      L2_2 = MrxTutorialManager.ShowMessage("[Tutorial.Grapple.Key1]", true)
      if L2_2 then
        L2_2 = MrxUtil.GetCharacterIdentity(A0_2)
        L3_2 = {}
        L3_2.mattias = "Mattias.Grapple01"
        L3_2.jennifer = "Jen.Grapple"
        L3_2.chris = "Chris.Grapple01"
        CueTable = L3_2
        L4_2 = {}
        L5_2 = {}
        L5_2[1] = CueTable[L2_2]
        L5_2[2] = A0_2
        L4_2[1] = L5_2
        L4_2[2] = TutorialCueCallback
        MrxVoSequence.Start(L4_2, nil, MrxVoSequence.knPriorityFreeplay)
        _uHideMessage = "InValid"
      end
    end
  end
end

DisableGrappleTriggered = L15_1
