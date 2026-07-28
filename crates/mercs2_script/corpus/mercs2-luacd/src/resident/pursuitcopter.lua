tEvents = tEvents or {}
inherit("VehicleBlippable")
import("MrxSupport")
tFlash = {
  255,
  255,
  255
}
sTexture = "temp_radar_icon_helicopter"
nSize = 5
tCopters = {}
tRetries = {}

function OnActivate(uGuid, uRuntimeOwner, iArg)
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, Start, {
    uGuid,
    uRuntimeOwner,
    iArg
  })
end

function Start(uGuid, uRuntimeOwner, iArg)
  self = getfenv()
  table.insert(tCopters, uGuid)
  local bDropZoneAdded = FindLZ(uGuid)
  if not bDropZoneAdded then
    Debug.Printf("Unable to perform LZ check, sending pursuit copter away")
    MrxSupport.GoHome(self, uGuid)
    table.remove(tCopters, 1)
  end
end

function FoundPosition(bFound, x, y, z)
  if not tCopters then
    Debug.Printf("ERROR! Received a callback for copter pursuit, but there is no active copter")
    return
  end
  local uGuid = tCopters[1]
  if not bFound then
    tRetries[uGuid] = tRetries[uGuid] or 0
    tRetries[uGuid] = tRetries[uGuid] + 1
    if tRetries[uGuid] > 3 then
      Debug.Printf("No valid LZ foundafter 3 attempts, sending copter home")
      MrxSupport.GoHome(self, uGuid)
      table.remove(tCopters, 1)
      return
    else
      Debug.Printf("No valid LZ found, trying again (attempt " .. tostring(tRetries[uGuid]) .. ")")
      local x, y, z = Object.GetPosition(Player.GetLocalCharacter())
      Ai.Goal({
        AIGuid = Vehicle.GetDriver(uGuid),
        Goal = "MoveTo",
        Location = {
          x,
          y + 50,
          z
        },
        Priority = "hiPri"
      })
      Event.Create(Event.TimerRelative, {3}, FindLZ, {uGuid})
      return
    end
  end
  self.LandGoal = Ai.Goal({
    AIGuid = Vehicle.GetDriver(uGuid),
    Goal = "HeliLand",
    Location = {
      x,
      y,
      z
    },
    Priority = "hiPri",
    Callback = AllOut,
    CallbackData = {self, uGuid}
  })
  local oInstance = self:Create(uGuid, uRuntimeOwner)
  table.remove(tCopters, 1)
end

function AllOut(self, uGuid, nState)
  if nState == 0 then
    Debug.Printf("Landing goal failed, sending pursuit copter away")
    MrxSupport.GoHome(self, uGuid)
    return
  end
  Ai.Deploy({
    Vehicle = uGuid,
    Role = "Passenger",
    Force = true,
    MaintainRotorSpeed = true,
    Callback = MrxSupport.GoHome,
    CallbackData = {self, uGuid}
  })
end

function FindLZ(uGuid)
  local x, y, z = Object.GetPosition(Player.GetLocalCharacter())
  local oData = {
    nHeightMax = 20,
    nInnerRadius = 6,
    nOuterRadius = 19,
    nInnerHeightTolerance = 1,
    nOuterHeightTolerance = 2.5
  }
  local bDropZoneAdded = Ai.TestDropZone({
    Callback = FoundPosition,
    Location = {
      x,
      y,
      z
    },
    InnerRadius = oData.nInnerRadius,
    InnerHeightTolerance = oData.nInnerHeightTolerance,
    OuterRadius = oData.nOuterRadius,
    OuterHeightTolerance = oData.nOuterHeightTolerance,
    HeightMax = oData.nHeightMax,
    SearchRadius = 40,
    Water = false
  })
  return bDropZoneAdded
end
