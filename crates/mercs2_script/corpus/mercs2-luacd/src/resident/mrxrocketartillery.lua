inherit("MrxSupport")
import("MrxSupportDesignatorSatellite")
import("MrxVoSequence")
import("MrxUtil")

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  local oDesignator = MrxSupportDesignatorSatellite:Create()
  oDesignator:SetMinigameSectors({
    {35, 55},
    {170, 190},
    {305, 325}
  })
  oNewSupport:SetDesignator(oDesignator)
  oDesignator.sAATestLevel = nil
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Fiona")
  oNewSupport:SetModuleName("MrxRocketArtillery")
  return oNewSupport
end

function DesignationCallback(self)
  local uHero = Player.GetCharacter(self.uOwner)
  local sAmmo = "Rocket Artillery Projectile"
  local nWidth = 100
  local nHeight = 50
  local nShells = 30
  local nTime = 8
  local nGrenadeX, nGrenadeY, nGrenadeZ = self:GetDesignator():GetTarget()
  local nHeroX, nHeroY, nHeroZ = Object.GetPosition(uHero)
  local nHeightVectorX, nHeightVectorY, nHeightVectorZ = nGrenadeX - nHeroX, nGrenadeY - nHeroY, nGrenadeZ - nHeroZ
  nHeightVectorX, nHeightVectorY, nHeightVectorZ = Math.Normalize(nHeightVectorX, nHeightVectorY, nHeightVectorZ)
  local nWidthVectorX, nWidthVectorY, nWidthVectorZ = -(nGrenadeZ - nHeroZ), 0, nGrenadeX - nHeroX
  nWidthVectorX, nWidthVectorY, nWidthVectorZ = Math.Normalize(nWidthVectorX, nWidthVectorY, nWidthVectorZ)
  for i = 1, nShells do
    local nWidthIncrement = nWidth / nShells
    local nHeightIncrement = nHeight
    local nXAdjust = -(math.randf() * nWidthIncrement - math.randf() * nWidthIncrement + ((nShells + 1) / 2 - i) * nWidthIncrement)
    local nZAdjust = -(math.randf() * nHeightIncrement - math.randf() * nHeightIncrement)
    local tData = {}
    tData.sAmmo = sAmmo
    tData.nTargetX = nGrenadeX + nWidthVectorX * nXAdjust + nHeightVectorX * nZAdjust
    tData.nTargetY = nGrenadeY + 250
    tData.nTargetZ = nGrenadeZ + nWidthVectorZ * nXAdjust + nHeightVectorZ * nZAdjust
    Event.Create(Event.TimerRelative, {
      3 + i * (nTime / nShells)
    }, TriggerFallingMissile, {
      tData,
      self.uOwner
    })
  end
  MrxVoSequence.Start({
    MrxUtil.GetRandomTableElement({
      "ChinaSoldier.cp1_artillery_roger",
      "ChinaSoldier01.Support.Artillery01",
      "ChinaSoldier01.Support.Artillery02",
      "ChinaSoldier01.Support.Incoming01",
      "ChinaSoldier01.Support.Incoming02",
      "ChinaSoldier01.Support.Incoming03"
    })
  }, nil, MrxVoSequence.knPriorityFreeplay, false)
end

function TriggerFallingMissile(tData, uPlayer)
  local uOrdnanceGuid = Airstrike.SpawnOrdnance(tData.sAmmo, tData.nTargetX, tData.nTargetY, tData.nTargetZ, 0, -100, 0, "impact", 1, uPlayer, ActivateDelay, {tData})
end
