inherit("MrxSupport")
import("MrxSupportDesignatorSatellite")

function Create(self, uOwnerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  oNewSupport.nTotalLines = 7
  oNewSupport.nRemainingLines = oNewSupport.nTotalLines
  oNewSupport.nTimeInterval = 0.35
  local oDesignator = MrxSupportDesignatorSatellite:Create()
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetRecruit("Fiona")
  oNewSupport:SetOwner(uOwnerGuid)
  oNewSupport:SetModuleName("MrxCarpetBomb")
  return oNewSupport
end

function DesignationCallback(self)
  local nAltitudeDifference = 100
  local nTargetX, nTargetY, nTargetZ, uDesignatorGuid = self:GetDesignator():GetTarget()
  self.nHeading = Camera.GetYaw(Player.GetCamera(self:GetOwner()))
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(-300, nAltitudeDifference, -1, self.uOwner)
  if nSpawnY < nTargetY + nAltitudeDifference then
    nSpawnY = nTargetY + nAltitudeDifference
  end
  local uJet = Airstrike.Flyby("Support Vehicle (B2)", nSpawnX, nSpawnZ, nTargetX, nTargetZ, nSpawnY, 100, DropBomb, {self})
  self.uJet = uJet
  MrxSupport.PlayAirstrikeVO(uJet, {""})
end

function DropBomb(oAirstrike)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(oAirstrike.uJet)
  local nNextX, nNextY, nNextZ
  nNextY = nSpawnY
  nNextX, nNextZ = Airstrike.SpawnCarpetBombLine(nSpawnX, nSpawnY, nSpawnZ, oAirstrike.nHeading, oAirstrike:GetOwner(), nil, 5, 15)
  oAirstrike.nNextX = nNextX
  oAirstrike.nNextY = nNextY + math.randi(10, 30)
  oAirstrike.nNextZ = nNextZ
  Event.Create(Event.TimerRelative, {
    oAirstrike.nTimeInterval
  }, NextExplosionCallback, {oAirstrike})
end

function NextExplosionCallback(oAirstrike)
  local nNextX, nNextZ
  nNextX, nNextZ = Airstrike.SpawnCarpetBombLine(oAirstrike.nNextX, oAirstrike.nNextY, oAirstrike.nNextZ, oAirstrike.nHeading, oAirstrike:GetOwner(), nil, 5, 15)
  oAirstrike.nNextX = nNextX
  oAirstrike.nNextZ = nNextZ
  oAirstrike.nRemainingLines = oAirstrike.nRemainingLines - 1
  if oAirstrike.nRemainingLines <= 0 then
    oAirstrike.nRemainingLines = oAirstrike.nTotalLines
  else
    Event.Create(Event.TimerRelative, {
      oAirstrike.nTimeInterval
    }, NextExplosionCallback, {oAirstrike})
  end
end

function explode()
  local x, y, z = Object.GetPosition(Player.GetLocalCharacter())
  Pg.Spawn("carpetbomb_explosion", x, y, z)
end
