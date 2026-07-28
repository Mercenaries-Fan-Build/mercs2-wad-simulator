inherit("MrxTutorial")
import("MrxTutorialManager")
import("MrxFactionManager")
local msg = "Collectibles tutorial."
local nCount = 1
local bActivated
local bMessageShowing = false
local msgtable = {
  "[Tutorial.Collectibles]",
  "[Tutorial.Collectibles2]"
}
local eventHandle

function GetMessage()
  return msg
end

function SetupActivationCriteria(self)
  if uEvent then
    Event.Delete(uEvent)
    uEvent = nil
  end
  uFilter = ObjectFilter.Create()
  ObjectFilter.SetFilter(uFilter, "SpareParts")
  uEvent = self:_CreateEvent(Event.ObjectProximity, {
    uFilter,
    Player.GetLocalCharacter(),
    "<",
    5,
    false,
    false
  }, ShowMessage, {self})
end

function ShowMessage(self)
  if not bActivated then
    bResult = self:ActivateTutorial(true)
    if bResult then
      bActivated = true
    end
  end
  if bActivated and not bMessageShowing then
    msg = msgtable[nCount]
    local bResult = MrxTutorialManager.UpdateCurrentTutorial(self, true)
    if eventHandle then
      Event.Delete(eventHandle)
      eventHandle = nil
    end
    eventHandle = self:_CreateEvent(Event.TimerRelative, {10}, HideMessage, {self})
    bMessageShowing = true
  end
end

function HideMessage(self)
  MrxTutorialManager.HideMessage(false, "Collectibles")
  bMessageShowing = false
  nCount = nCount + 1
  if 2 < nCount then
    if eventHandle then
      Event.Delete(eventHandle)
      eventHandle = nil
    end
    eventHandle = self:_CreateEvent(Event.TimerRelative, {10}, EndTutorial, {self, true})
  else
    SetupActivationCriteria(self)
  end
end

function SetupCompletionCriteria(self)
end

function EndTutorial(self, bComplete)
  if eventHandle then
    Event.Delete(eventHandle)
    eventHandle = nil
  end
  bMessageShowing = false
  MrxTutorial.EndTutorial(self, bComplete)
end
