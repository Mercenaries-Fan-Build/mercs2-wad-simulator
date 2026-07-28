inherit("MrxTutorial")
import("MrxTutorialManager")
import("MrxFactionManager")
local msg = "Disguise tutorial."
local nLastDisguiseState = -2
local bDisguised2Not = false
local bNot2Disgusied = false
local nCount = 0

function GetMessage()
  return msg
end

local oSelf

function SetupActivationCriteria(self)
  local uRider = Player.GetLocalCharacter()
  Player.VehicleDisguise({Player = uRider, Callback = DisguiseChangedCallback})
  oSelf = self
  oSelf._bDoOnce = nil
end

local _tEvents = {}

function DisguiseChangedCallback(playerGuid, nDisguiseState, uFaction)
  if not Player.GetVehicleDisguise() then
    return
  end
  local uRider = Player.GetLocalCharacter()
  local uVehicle = Vehicle.GetFromRider(uRider)
  if not uVehicle then
    return
  end
  local sFactionIcon = MrxFactionManager.GetInlineIcon(MrxFactionManager.GetFactionStringAbbrev(uVehicle))
  local bDisguiseState = Player.GetVehicleDisguiseState({Player = uRider})
  local bUpdate = false
  if tostring(bDisguiseState) == "true" then
    msg = "[Tutorial.VehicleDisguise.Key1:" .. sFactionIcon .. "]"
    nLastDisguiseState = 1
    bUpdate = true
  elseif tostring(bDisguiseState) == "false" then
    msg = "[Tutorial.VehicleDisguise.Key2:" .. sFactionIcon .. "]"
    nLastDisguiseState = 0
    bUpdate = true
  end
  if not bUpdate then
    return
  end
  if not oSelf._bDoOnce then
    local bResult = oSelf:ActivateTutorial(true)
    if bResult then
      oSelf._bDoOnce = true
      if nLastDisguiseState == 1 then
        bDisplayedMsgOne = true
      else
        bDisplayedMsgTwo = true
      end
    end
  else
    local bResult = MrxTutorialManager.UpdateCurrentTutorial(oSelf, true)
    if nLastDisguiseState == 1 then
      bDisplayedMsgOne = true
    else
      bDisplayedMsgTwo = true
      nCount = nCount + 1
    end
  end
  if bDisplayedMsgOne == true and bDisplayedMsgTwo == true and 3 <= nCount then
    Event.Create(Event.TimerRelative, {6}, EndTutorial, {oSelf, true})
  else
    if _oHideMessageEvent then
      Event.Delete(_oHideMessageEvent)
    end
    _oHideMessageEvent = Event.Create(Event.TimerRelative, {10}, HideDisguiseMessage, {})
  end
end

function HideDisguiseMessage()
  MrxTutorialManager.HideMessage(false, "VehicleDisguise")
end

function SetupCancellationCriteria(self)
  local uRider = Player.GetLocalCharacter()
  local uVehicle = Vehicle.GetFromRider(uRider)
  self._oCancelEvent = self:_CreateEvent(Event.ObjectInSeat, {
    uRider,
    uVehicle,
    "A",
    "X"
  }, self.EndTutorial, {self, false})
end

function EndTutorial(self, bComplete)
  local uRider = Player.GetLocalCharacter()
  nLastDisguiseState = -1
  if bComplete == true then
    Player.VehicleDisguise({Player = uRider, Remove = true})
  end
  MrxTutorial.EndTutorial(self, bComplete)
end

function SetupCompletionCriteria(self)
end
